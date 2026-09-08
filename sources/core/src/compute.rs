//! Running a node's work off the UI thread.

use std::{collections::VecDeque, sync::mpsc, thread};

use crate::{Action, NodeUid};

pub struct ComputeTask {
    requester: NodeUid,
    task: Box<dyn (FnOnce() -> Vec<Action>) + Send>,
}

impl ComputeTask {
    pub fn new(requester: NodeUid, task: impl (FnOnce() -> Vec<Action>) + Send + 'static) -> Self {
        Self {
            requester,
            task: Box::new(task),
        }
    }
}

/// Which dispatch a worker is reporting on, so a report cannot be mistaken for a later one that reused the thread.
type RunId = u64;

/// Something for the scheduler to do, in the order it was asked. Channel order is semantic.
enum Request {
    Run(ComputeTask),
    /// Throw away everything this node has asked for so far.
    CancelAllFor(NodeUid),
    /// A worker with nothing to do, waiting to be given something.
    Free(oneshot::Sender<Assignment>),
    /// A worker reporting that a dispatch is over, cancelled or not.
    Finished(RunId),
    /// The workspace is gone; wind the pool up. Sent by the handle's own `Drop`.
    Shutdown,
}

pub struct ComputeSchedulerHandle {
    requests: mpsc::Sender<Request>,
}

impl ComputeSchedulerHandle {
    /// A handle attached to no scheduler, with submission dropped on the floor. For detached workspaces.
    pub fn disconnected() -> Self {
        let (requests, _) = mpsc::channel();
        Self { requests }
    }

    pub fn submit_task(&self, task: ComputeTask) {
        let _ = self.requests.send(Request::Run(task));
    }

    pub fn cancel_all_tasks_for(&self, node: NodeUid) {
        let _ = self.requests.send(Request::CancelAllFor(node));
    }
}

impl Drop for ComputeSchedulerHandle {
    fn drop(&mut self) {
        // The workspace this pool was working for has gone.
        let _ = self.requests.send(Request::Shutdown);
    }
}

pub struct ComputeScheduler {
    /// Everything the scheduler acts on, in the order it happened
    incoming: mpsc::Receiver<Request>,

    /// Tasks that have not yet been started
    queued_tasks: VecDeque<ComputeTask>,
    /// Workers waiting to be given one
    free_workers: VecDeque<oneshot::Sender<Assignment>>,
    /// Dispatches still running, and the means to throw each one away
    active: Vec<ActiveRun>,
    /// Names the next dispatch
    next_run: RunId,
}

impl ComputeScheduler {
    pub fn spawn(action_queue: mpsc::Sender<Action>) -> ComputeSchedulerHandle {
        let avail_threads = thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(8);

        // 1 (main) thread for UI, 1 thread to run the scheduler
        // Spawn 1 compute thread minimum
        let num_workers = avail_threads.saturating_sub(2).max(1);

        let (requests, incoming) = mpsc::channel();

        // Spawn all workers. Each holds a sender of its own.
        for _ in 0..num_workers {
            let requests = requests.clone();
            let action_queue = action_queue.clone();
            thread::spawn(|| Self::worker_compute_loop(requests, action_queue));
        }

        thread::spawn(|| {
            let mut scheduler = Self {
                incoming,
                queued_tasks: VecDeque::new(),
                free_workers: VecDeque::new(),
                active: Vec::new(), // No tasks running yet
                next_run: 0,
            };
            scheduler.drive();
        });

        ComputeSchedulerHandle { requests }
    }

    /// Take work as it comes, and cost nothing while there is none.
    fn drive(&mut self) {
        loop {
            // Nothing to do until somebody says so.
            let Ok(first) = self.incoming.recv() else {
                return;
            };
            if !self.take(first) {
                return;
            }
            // Whatever else piled up behind it, before deciding anything.
            while let Ok(request) = self.incoming.try_recv() {
                if !self.take(request) {
                    return;
                }
            }
            self.dispatch_all();
        }
    }

    /// Fold one request into what the scheduler knows; [`false`] to stop.
    fn take(&mut self, request: Request) -> bool {
        match request {
            Request::Run(task) => self.queued_tasks.push_back(task),
            Request::CancelAllFor(node) => self.cancel(node),
            Request::Free(worker) => self.free_workers.push_back(worker),
            // Over is over: there is nothing left to cancel.
            Request::Finished(run) => self.active.retain(|active| active.run != run),
            // Returning drops the receiver, which is how the workers find out.
            Request::Shutdown => return false,
        }
        true
    }

    /// Drop `node`'s queued work and ask for its running work to be thrown away.
    fn cancel(&mut self, node: NodeUid) {
        self.queued_tasks.retain(|task| task.requester != node);
        self.active.retain(|active| {
            if active.requester != node {
                return true;
            }
            // A run that ended between its report and this sweep has already dropped its end.
            let _ = active.kill.send(());
            false
        });
    }

    /// Pair queued work with waiting workers until one side runs out.
    fn dispatch_all(&mut self) {
        while !self.queued_tasks.is_empty() && !self.free_workers.is_empty() {
            let (Some(task), Some(worker)) =
                (self.queued_tasks.pop_front(), self.free_workers.pop_front())
            else {
                return;
            };

            let run = self.next_run;
            self.next_run += 1;
            let (kill, killed) = mpsc::channel();
            // Read before the task goes: cancellation is by node, while the task is about to belong to the worker.
            let requester = task.requester;

            // A worker that has gone away takes its task with it.
            if worker.send(Assignment { run, task, killed }).is_err() {
                continue;
            }
            self.active.push(ActiveRun {
                run,
                requester,
                kill,
            });
        }
    }

    fn worker_compute_loop(requests: mpsc::Sender<Request>, action_queue: mpsc::Sender<Action>) {
        loop {
            // Offer this thread.
            let (respond, assignment) = oneshot::channel();

            // Scheduler has hung up.
            if requests.send(Request::Free(respond)).is_err() {
                return;
            }
            let Ok(Assignment { run, task, killed }) = assignment.recv() else {
                return;
            };

            let produced = (task.task)();

            // Cancelled while it ran: the work is finished and thrown away.
            if killed.try_recv().is_err() {
                for action in produced {
                    let _ = action_queue.send(action);
                }
            }

            // Report in, so this run stops being one the scheduler can cancel.
            let _ = requests.send(Request::Finished(run));
        }
    }
}

/// One dispatch, as the worker receives it.
struct Assignment {
    run: RunId,
    task: ComputeTask,
    /// Signalled when the results should be thrown away rather than committed.
    killed: mpsc::Receiver<()>,
}

/// One dispatch, as the scheduler remembers it.
struct ActiveRun {
    run: RunId,
    requester: NodeUid,
    kill: mpsc::Sender<()>,
}
