use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RunnableStatus {
    Running,
    Stopped,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunnableStopped {
    pub(crate) name: &'static str,
    pub(crate) status: RunnableStatus,
}

#[derive(Clone)]
pub(crate) struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    runnables: Mutex<BTreeMap<&'static str, RunnableStatus>>,
    changed: watch::Sender<u64>,
}

impl Default for AppState {
    fn default() -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            inner: Arc::new(Inner {
                runnables: Mutex::new(BTreeMap::new()),
                changed,
            }),
        }
    }
}

impl AppState {
    pub(crate) fn spawn<F>(&self, name: &'static str, future: F) -> anyhow::Result<()>
    where
        F: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let runnable = self.register(name)?;
        let task = tokio::spawn(future);

        tokio::spawn(async move {
            let status = match task.await {
                Ok(Ok(())) => RunnableStatus::Stopped,
                Ok(Err(error)) => RunnableStatus::Failed(error.to_string()),
                Err(error) => RunnableStatus::Failed(format!("task panicked: {error}")),
            };
            runnable.set_status(status);
        });

        Ok(())
    }

    pub(crate) async fn wait_for_runnable_stop(&self) -> RunnableStopped {
        let mut changed = self.inner.changed.subscribe();

        loop {
            if let Some(stopped) = self.stopped_runnable() {
                return stopped;
            }

            changed
                .changed()
                .await
                .expect("application state notification channel cannot close");
        }
    }

    fn register(&self, name: &'static str) -> anyhow::Result<Runnable> {
        let mut runnables = self
            .inner
            .runnables
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if runnables.insert(name, RunnableStatus::Running).is_some() {
            anyhow::bail!("runnable '{name}' is already registered");
        }
        drop(runnables);
        self.notify_changed();

        Ok(Runnable {
            name,
            app_state: self.clone(),
        })
    }

    fn stopped_runnable(&self) -> Option<RunnableStopped> {
        let runnables = self
            .inner
            .runnables
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        runnables.iter().find_map(|(&name, status)| {
            if *status == RunnableStatus::Running {
                None
            } else {
                Some(RunnableStopped {
                    name,
                    status: status.clone(),
                })
            }
        })
    }

    fn set_status(&self, name: &'static str, status: RunnableStatus) {
        let mut runnables = self
            .inner
            .runnables
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        runnables.insert(name, status);
        drop(runnables);
        self.notify_changed();
    }

    fn notify_changed(&self) {
        self.inner
            .changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }
}

struct Runnable {
    name: &'static str,
    app_state: AppState,
}

impl Runnable {
    fn set_status(self, status: RunnableStatus) {
        self.app_state.set_status(self.name, status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_a_runnable_error() {
        let state = AppState::default();
        state
            .spawn("test", async { Err(anyhow::anyhow!("simulated error")) })
            .unwrap();

        let stopped = state.wait_for_runnable_stop().await;

        assert_eq!(stopped.name, "test");
        assert_eq!(
            stopped.status,
            RunnableStatus::Failed("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn reports_a_runnable_panic() {
        let state = AppState::default();
        state
            .spawn("test", async {
                panic!("simulated panic");
                #[allow(unreachable_code)]
                Ok(())
            })
            .unwrap();

        let stopped = state.wait_for_runnable_stop().await;

        assert_eq!(stopped.name, "test");
        assert!(matches!(
            stopped.status,
            RunnableStatus::Failed(ref error) if error.contains("task panicked")
        ));
    }
}
