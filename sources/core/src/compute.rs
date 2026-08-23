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

pub struct ComputeSchedulerHandle {
    task_sender: mpsc::Sender<ComputeTask>,
    cancellation_sender: mpsc::Sender<NodeUid>,
}

impl ComputeSchedulerHandle {
    pub fn submit_task(&self, task: ComputeTask) {
        let _ = self.task_sender.send(task);
    }

    pub fn cancel_all_tasks_for(&self, node: NodeUid) {
        let _ = self.cancellation_sender.send(node);
    }
}

pub struct ComputeScheduler {
    /// Continually pushed into [`Self::queued_tasks`]
    incoming_tasks: mpsc::Receiver<ComputeTask>,
    /// Removes tasks from [`Self::queued_tasks`] and cancels relevant threads in [`Self::active_threads`]
    incoming_cancellation_signals: mpsc::Receiver<NodeUid>,

    /// Tasks that have not yet been started
    queued_tasks: VecDeque<ComputeTask>,
    /// Threads available to do work
    free_workers: mpsc::Receiver<FreeThreadInfo>, // Only communication channel shared with workers

    /// Threads currently doing work
    active_workers: Vec<ActiveWorker>,
}

impl ComputeScheduler {
    pub fn spawn(action_queue: mpsc::Sender<Action>) -> ComputeSchedulerHandle {
        let avail_threads = thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(8);

        // 1 (main) thread for UI, 1 thread to run the scheduler
        // Spawn 1 compute thread minimum
        let num_workers = avail_threads.saturating_sub(2).max(1);

        // Scheduler <-> worker channel
        let (free_thread_sender, free_thread_recv) = mpsc::channel();

        // Spawn all workers
        for _ in 0..num_workers {
            let free_thread_tx = free_thread_sender.clone();
            let action_queue = action_queue.clone();

            thread::spawn(|| Self::worker_compute_loop(free_thread_tx, action_queue));
        }

        // Scheduler thread
        let (task_tx, task_recv) = mpsc::channel();
        let (cancel_tx, cancel_recv) = mpsc::channel();
        thread::spawn(|| {
            let mut scheduler = Self {
                incoming_tasks: task_recv,
                incoming_cancellation_signals: cancel_recv,

                queued_tasks: VecDeque::new(),
                free_workers: free_thread_recv,

                active_workers: Vec::new(), // No tasks running yet
            };
            scheduler.drive();
        });

        ComputeSchedulerHandle {
            task_sender: task_tx,
            cancellation_sender: cancel_tx,
        }
    }

    fn drive(&mut self) {
        loop {
            // Check if any workers have finished their work
            self.active_workers.retain(|worker| {
                if worker.complete_recv.try_recv().is_ok() {
                    false // Finished!
                } else {
                    true
                }
            });

            // Check if a new task has come in
            if let Ok(task) = self.incoming_tasks.try_recv() {
                self.queued_tasks.push_back(task);
            }

            // Check if any nodes have cancelled their work
            if let Ok(id) = self.incoming_cancellation_signals.try_recv() {
                // Remove queued tasks for this node
                self.queued_tasks.retain(|task| task.requester != id);
                // Send kill signals to active workers for this node
                self.active_workers.retain(|worker| {
                    if worker.requester == id {
                        worker.kill_tx.send(()).expect("Sends should not fail"); // Graceful shutdown
                        false // Do not retain
                    } else {
                        true
                    }
                });
            }

            while !self.queued_tasks.is_empty() {
                if let Ok(worker) = self.free_workers.try_recv() {
                    // Compute available to do this task!
                    let task = self
                        .queued_tasks
                        .pop_front()
                        .expect("Queue should not be empty");

                    // Scheduler <-> worker communication channels
                    let (kill_tx, kill_recv) = mpsc::channel();
                    let (complete_tx, complete_recv) = mpsc::channel();

                    let worker_info = ActiveWorker {
                        requester: task.requester,
                        kill_tx,
                        complete_recv,
                    };
                    self.active_workers.push(worker_info);

                    worker
                        .response
                        .send(ComputeTaskContext {
                            ctask: task,
                            kill_recv,
                            complete_tx,
                        })
                        .expect("Sends should not fail");
                }
            }
        }
    }

    fn worker_compute_loop(
        free_thread_tx: mpsc::Sender<FreeThreadInfo>,
        action_queue: mpsc::Sender<Action>,
    ) -> ! {
        loop {
            // Inform the scheduler that this thread is ready for work
            let (tx, rx) = oneshot::channel();
            free_thread_tx
                .send(FreeThreadInfo { response: tx })
                .expect("Scheduler should not be dropped");

            if let Ok(ctx) = rx.recv() {
                // Do the work of the task
                let res = (ctx.ctask.task)();

                // Should we cancel before committing the results?
                let should_cancel = ctx.kill_recv.try_recv().is_ok();
                if !should_cancel {
                    for action in res {
                        let _ = action_queue.send(action);
                    }
                }

                // Inform the scheduler that this task has been completed
                ctx.complete_tx.send(()).expect("Sends should not fail");
            }
        }
    }
}

struct FreeThreadInfo {
    /// A mailbox for a task and cancellation channel
    response: oneshot::Sender<ComputeTaskContext>,
}

/// Passed to the worker
struct ComputeTaskContext {
    ctask: ComputeTask,
    kill_recv: mpsc::Receiver<()>,
    complete_tx: mpsc::Sender<()>,
}

/// Held by the scheduler
struct ActiveWorker {
    requester: NodeUid,
    kill_tx: mpsc::Sender<()>,
    complete_recv: mpsc::Receiver<()>,
}
