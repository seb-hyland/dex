//! What the compute pool costs, and what it does with the order it is asked in.
//!
//! Both properties here are invisible until something is slow or something is
//! stale, which is exactly why they need pinning. The pool used to spin: every
//! workspace ever built kept a core busy for the life of the process, so a test
//! suite that made a hundred of them ran the last one about ten times slower
//! than the first. And runs and cancellations used to travel down separate
//! channels, where their order was whichever the scheduler happened to look at
//! first — survivable only while it looked millions of times a second.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dex_core::prelude::*;
use dex_nodes::primitives::nothing::Nothing;

/// Wait for `f`, turning the queue over, or give up and say so.
fn within(ws: &mut Workspace, limit: Duration, mut f: impl FnMut(&Workspace) -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        ws.process_pending();
        if f(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    false
}

/// A fixed slice of arithmetic, timed. The unit is deliberately arbitrary: what
/// matters is the same work measured twice under different conditions.
fn fixed_work() -> Duration {
    let start = Instant::now();
    let mut acc = 0u64;
    for i in 0..20_000_000u64 {
        acc = acc.wrapping_add(i ^ acc);
    }
    std::hint::black_box(acc);
    start.elapsed()
}

/**
    An idle workspace costs nothing to keep.

    The measurement is a ratio rather than a number, so it says the same thing
    on a busy laptop as on a quiet one: the same arithmetic, once alone and once
    beside forty idle workspaces. Spinning schedulers made that roughly forty
    times more expensive on a ten-core machine; blocking ones make it free, and
    the margin here is wide enough that only a return to spinning trips it.
*/
#[test]
fn idle_workspaces_cost_nothing() {
    const IDLE_WORKSPACES: usize = 40;

    // Warm up, so the first sample is not paying for whatever the process has
    // not done yet.
    let _ = fixed_work();
    let alone = fixed_work().min(fixed_work());

    let mut kept = Vec::new();
    for _ in 0..IDLE_WORKSPACES {
        kept.push(Workspace::new_empty());
    }

    let crowded = fixed_work().min(fixed_work());
    let ratio = crowded.as_secs_f64() / alone.as_secs_f64();
    assert!(
        ratio < 2.0,
        "{IDLE_WORKSPACES} idle workspaces made the same work {ratio:.1}x slower \
         ({alone:.1?} alone, {crowded:.1?} beside them) — the pool is busy-waiting"
    );
    drop(kept);
}

/**
    A workspace's threads go with it.

    Nothing observes a thread directly; what is observed is that the pool stops
    taking work, which is the same fact from the outside. Without this a long
    session leaks a pool per workspace ever opened.
*/
#[test]
fn a_dropped_workspace_winds_its_pool_up() {
    let ran = Arc::new(AtomicUsize::new(0));

    let mut ws = Workspace::new_empty();
    let counter = Arc::clone(&ran);
    ws.submit_task(ComputeTask::new(NodeUid::nil(), move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }));
    assert!(
        within(&mut ws, Duration::from_secs(5), |_| ran
            .load(Ordering::SeqCst)
            == 1),
        "the pool runs what it is given while its workspace is alive"
    );

    drop(ws);
    // The pool is wound up asynchronously, so give it a moment before asking.
    std::thread::sleep(Duration::from_millis(50));

    // Nothing here can prove a thread exited, but a pool that has stopped
    // taking work is a pool that has stopped, and it is what callers see.
    assert_eq!(
        ran.load(Ordering::SeqCst),
        1,
        "and nothing else ran once the workspace was gone"
    );
}

/**
    A cancellation cannot reach across the submission that followed it.

    This is the shape every recomputing node uses: cancel whatever I asked for
    before, then ask for this instead. Read in the wrong order it cancels the
    replacement, and the node simply never computes — which is what happened the
    moment the scheduler was allowed to sleep between the two messages, because
    they were travelling down channels of their own.
*/
#[test]
fn a_cancellation_does_not_eat_the_task_submitted_after_it() {
    let node = NodeUid::mint();

    // Repeated, because the failure is a race: one attempt proves very little,
    // and the broken ordering lost the task nearly every time.
    for attempt in 0..200 {
        let ran = Arc::new(AtomicUsize::new(0));
        let mut ws = Workspace::new_empty();

        ws.cancel_all_tasks_for(node);
        let counter = Arc::clone(&ran);
        ws.submit_task(ComputeTask::new(node, move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        }));

        assert!(
            within(&mut ws, Duration::from_secs(5), |_| ran
                .load(Ordering::SeqCst)
                == 1),
            "attempt {attempt}: the task submitted after the cancellation never ran"
        );
    }
}

/**
    A cancellation *does* reach the work asked for before it.

    The other half, and the reason the ordering cannot simply be ignored: what
    a node cancels is its own previous attempt, whose results are about to be
    wrong. Here the work is already running when the cancellation lands, so it
    is the committed actions that have to be thrown away rather than the task.
*/
#[test]
fn a_cancellation_throws_away_the_run_it_caught() {
    let node = NodeUid::mint();
    let started = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(AtomicUsize::new(0));

    let mut ws = Workspace::new_empty();
    let root = ws.action_handle().insert_node_dyn(Arc::new(Nothing));
    ws.process_pending();
    ws.set_root(root);

    let (seen, go) = (Arc::clone(&started), Arc::clone(&release));
    ws.submit_task(ComputeTask::new(node, move || {
        seen.fetch_add(1, Ordering::SeqCst);
        // Hold the thread until the cancellation has certainly been sent.
        while go.load(Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        // Actions are buffered and handed back at the end, exactly as a
        // lambda's transform does it.
        let (handle, actions) = WorkspaceActionHandle::buffered();
        handle.insert_node_at_dyn(NodeUid::mint(), Arc::new(Nothing));
        drop(handle);
        actions.try_iter().collect()
    }));

    assert!(
        within(&mut ws, Duration::from_secs(5), |_| started
            .load(Ordering::SeqCst)
            == 1),
        "the task started"
    );

    let before = ws.live_ids().len();
    ws.cancel_all_tasks_for(node);
    release.store(1, Ordering::SeqCst);

    // Long enough that an uncancelled action would certainly have landed.
    for _ in 0..200 {
        ws.process_pending();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ws.live_ids().len(),
        before,
        "a cancelled run's actions are thrown away rather than committed"
    );
}
