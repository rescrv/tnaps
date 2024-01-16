use std::collections::LinkedList;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{Builder, JoinHandle};

/// A unit of work is a Send-able function that gets called exactly once.
pub type WorkUnit = dyn FnOnce() + Send;

/////////////////////////////////////////// Coordination ///////////////////////////////////////////

#[derive(Default)]
struct Coordination {
    shutdown: AtomicBool,
    work: Mutex<LinkedList<Box<WorkUnit>>>,
    can_work: Condvar,
}

impl Coordination {
    fn enqueue(&self, work_unit: Box<WorkUnit>) {
        let mut list = LinkedList::default();
        list.push_front(work_unit);
        {
            let mut work = self.work.lock().unwrap();
            work.append(&mut list);
        }
        self.can_work.notify_one();
    }

    fn worker(self: Arc<Self>) {
        loop {
            let work_unit = {
                let mut work = self.work.lock().unwrap();
                while work.is_empty() && !self.shutdown.load(Ordering::Relaxed) {
                    work = self.can_work.wait(work).unwrap();
                }
                if work.is_empty() && self.shutdown.load(Ordering::Relaxed) {
                    return;
                }
                // SAFETY(rescrv):  We checked work.is_empty() and hold a mutex.
                // Shutdown is a stable property false->true, so it will not race.
                work.pop_front().unwrap()
            };
            self.do_work(work_unit);
        }
    }

    fn do_work(&self, work_unit: Box<WorkUnit>) {
        work_unit()
    }
}

//////////////////////////////////////////// ThreadPool ////////////////////////////////////////////

/// ThreadPool provides a pool of threads waiting to do work.  The thread-pool is intended to be a
/// long-lived (lifetime of the process) object that gets used for parallel systems and parallel
/// ComponentCollection::apply calls.
pub struct ThreadPool {
    coordination: Arc<Coordination>,
    threads: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    /// Create a new thread pool with num-threads identified by `name:num`.
    pub fn new(name: &str, num: usize) -> Self {
        let coordination = Arc::new(Coordination::default());
        let mut threads = Vec::with_capacity(num);
        for _ in 0..num {
            let coordination = Arc::clone(&coordination);
            let thread = Builder::new()
                .name(format!("{}:{}", name, num))
                .stack_size(2 * 1024 * 1024)
                .spawn(|| coordination.worker())
                .expect("thread should always spawn");
            threads.push(thread);
        }
        Self {
            coordination,
            threads,
        }
    }

    /// Enqueue a unit of work on the threadpool.  It is the caller's responsibility to make the
    /// unit of work signal completion if said completion-signaling is necessary for correctness.
    pub fn enqueue(&self, work_unit: Box<WorkUnit>) {
        self.coordination.enqueue(work_unit);
    }

    /// Shutdown the threadpool.  This will wait for all enqueued work to finish before it returns.
    pub fn shutdown(self) {
        self.coordination.shutdown.store(true, Ordering::Relaxed);
        self.coordination.can_work.notify_all();
        for jh in self.threads.into_iter() {
            let _ = jh.join();
        }
    }
}
