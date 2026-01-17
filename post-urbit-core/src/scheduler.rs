use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{Mutex, oneshot};
use tokio::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub name: String,
    pub interval_seconds: Option<u64>,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub status: String,
    pub error_count: u32,
    pub last_error: Option<String>,
}

struct TaskRecord {
    info: TaskInfo,
    cancel: Option<oneshot::Sender<()>>,
}

#[derive(Clone)]
pub struct Scheduler {
    tasks: Arc<Mutex<HashMap<String, TaskRecord>>>,
}

pub struct TaskHandle {
    name: String,
}

type TaskFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

type TaskHandler = Arc<dyn Fn() -> TaskFuture + Send + Sync>;

impl Scheduler {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn schedule<F, Fut>(
        &self,
        name: &str,
        interval: Duration,
        handler: F,
    ) -> TaskHandle
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let name = name.to_string();
        let name_for_task = name.clone();
        let handler: TaskHandler = Arc::new(move || Box::pin(handler()));
        let (tx, mut rx) = oneshot::channel();
        let tasks = self.tasks.clone();
        let interval_seconds = interval.as_secs();

        {
            let mut locked = tasks.lock().await;
            locked.insert(
                name.clone(),
                TaskRecord {
                    info: TaskInfo {
                        name: name.clone(),
                        interval_seconds: Some(interval_seconds),
                        last_run: None,
                        next_run: Some((Utc::now() + chrono::Duration::seconds(interval_seconds as i64)).to_rfc3339()),
                        status: "scheduled".to_string(),
                        error_count: 0,
                        last_error: None,
                    },
                    cancel: Some(tx),
                },
            );
        }

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval_at(Instant::now() + interval, interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let mut locked = tasks.lock().await;
                        let Some(record) = locked.get_mut(&name_for_task) else { break; };
                        record.info.status = "running".to_string();
                        drop(locked);

                        let result = handler().await;

                        let mut locked = tasks.lock().await;
                        let Some(record) = locked.get_mut(&name_for_task) else { break; };
                        record.info.last_run = Some(Utc::now().to_rfc3339());
                        record.info.next_run = Some((Utc::now() + chrono::Duration::seconds(interval_seconds as i64)).to_rfc3339());
                        record.info.status = "scheduled".to_string();
                        if let Err(err) = result {
                            record.info.error_count += 1;
                            record.info.last_error = Some(err);
                        }
                    }
                    _ = &mut rx => {
                        let mut locked = tasks.lock().await;
                        if let Some(record) = locked.get_mut(&name_for_task) {
                            record.info.status = "cancelled".to_string();
                        }
                        break;
                    }
                }
            }
        });

        TaskHandle { name }
    }

    pub async fn run_once<F, Fut>(&self, name: &str, handler: F) -> TaskHandle
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let name = name.to_string();
        let name_for_task = name.clone();
        let handler: TaskHandler = Arc::new(move || Box::pin(handler()));
        let tasks = self.tasks.clone();

        {
            let mut locked = tasks.lock().await;
            locked.insert(
                name.clone(),
                TaskRecord {
                    info: TaskInfo {
                        name: name.clone(),
                        interval_seconds: None,
                        last_run: None,
                        next_run: None,
                        status: "scheduled".to_string(),
                        error_count: 0,
                        last_error: None,
                    },
                    cancel: None,
                },
            );
        }

        tokio::spawn(async move {
            let result = handler().await;
            let mut locked = tasks.lock().await;
            if let Some(record) = locked.get_mut(&name_for_task) {
                record.info.last_run = Some(Utc::now().to_rfc3339());
                record.info.status = "completed".to_string();
                if let Err(err) = result {
                    record.info.error_count += 1;
                    record.info.last_error = Some(err);
                }
            }
        });

        TaskHandle { name }
    }

    pub async fn cancel(&self, handle: TaskHandle) {
        let mut locked = self.tasks.lock().await;
        if let Some(record) = locked.get_mut(&handle.name) {
            if let Some(cancel) = record.cancel.take() {
                let _ = cancel.send(());
            }
        }
    }

    pub async fn list_tasks(&self) -> Vec<TaskInfo> {
        let locked = self.tasks.lock().await;
        locked.values().map(|record| record.info.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scheduler_runs_once() {
        let scheduler = Scheduler::new();
        let _handle = scheduler.run_once("one", || async { Ok(()) }).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let tasks = scheduler.list_tasks().await;
        assert!(tasks.iter().any(|task| task.name == "one"));
    }
}
