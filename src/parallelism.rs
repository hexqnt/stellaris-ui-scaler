use std::num::NonZeroUsize;

/// Бюджет логических CPU с учётом ограничений ОС и текущего пула Rayon.
#[derive(Clone, Copy)]
pub struct CpuBudget(NonZeroUsize);

impl CpuBudget {
    pub fn current() -> Self {
        let available = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
        Self(
            available
                .min(NonZeroUsize::new(rayon::current_num_threads()).unwrap_or(NonZeroUsize::MIN)),
        )
    }

    pub const fn threads(self) -> usize {
        self.0.get()
    }

    pub const fn per_task(self, tasks: NonZeroUsize) -> NonZeroUsize {
        match NonZeroUsize::new(self.0.get() / tasks.get()) {
            Some(threads) => threads,
            None => NonZeroUsize::MIN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_budget_respects_the_callers_rayon_pool_and_os_limit() {
        let available = std::thread::available_parallelism()
            .unwrap_or(NonZeroUsize::MIN)
            .get();
        for threads in [1, 3, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| assert_eq!(CpuBudget::current().threads(), threads.min(available)));
        }
    }

    #[test]
    fn task_budgets_cover_small_and_large_cpu_counts_without_oversubscription() {
        for cpus in [1, 2, 3, 8, 16, 32, 64, 128] {
            let budget = CpuBudget(NonZeroUsize::new(cpus).unwrap());
            assert_eq!(budget.per_task(NonZeroUsize::MIN).get(), cpus);
            for tasks in 1..=cpus {
                let threads = budget.per_task(NonZeroUsize::new(tasks).unwrap()).get();
                assert!(threads * tasks <= cpus);
                assert!(cpus - threads * tasks < tasks);
            }
        }
    }
}
