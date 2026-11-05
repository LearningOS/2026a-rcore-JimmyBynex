//! Bookkeeping tables for deadlock detection

use alloc::vec;
use alloc::vec::Vec;

/// Resource tables used by deadlock detection.
/// One instance for mutexes, one for semaphores.
pub struct DeadlockTable {
    /// available[res_id]: free count of each resource
    pub available: Vec<usize>,
    /// allocation[tid][res_id]: how many each thread holds
    pub allocation: Vec<Vec<usize>>,
    /// need[tid][res_id]: how many each thread is waiting for
    pub need: Vec<Vec<usize>>,
}

impl DeadlockTable {
    /// Create empty tables
    pub fn new() -> Self {
        Self {
            available: Vec::new(),
            allocation: Vec::new(),
            need: Vec::new(),
        }
    }

    /// Grow the tables so that row `tid` and column `res_id` exist
    pub fn ensure(&mut self, tid: usize, res_id: usize) {
        if res_id >= self.available.len() {
            self.available.resize(res_id + 1, 0);
        }
        if tid >= self.allocation.len() {
            self.allocation.resize(tid + 1, Vec::new());
            self.need.resize(tid + 1, Vec::new());
        }
        let res_count = self.available.len();
        for row in self.allocation.iter_mut().chain(self.need.iter_mut()) {
            row.resize(res_count, 0);
        }
    }

    /// Safety check: can every thread finish in some order?
    /// true = safe, false = deadlock
    pub fn is_safe(&self) -> bool {
        let thread_count = self.allocation.len();
        let mut work = self.available.clone();
        let mut finish = vec![false; thread_count];
        loop {
            // find an unfinished thread whose need can be met by work
            let found = (0..thread_count).find(|&tid| {
                !finish[tid]
                    && self.need[tid]
                        .iter()
                        .zip(work.iter())
                        .all(|(need, free)| need <= free)
            });
            match found {
                Some(tid) => {
                    // pretend it finishes and returns everything it holds
                    for (free, held) in work.iter_mut().zip(self.allocation[tid].iter()) {
                        *free += held;
                    }
                    finish[tid] = true;
                }
                None => break,
            }
        }
        finish.iter().all(|&done| done)
    }
}
