use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{Mutex, RwLock};

use crate::{
    db::Database,
    error::{AppError, AppResult},
};

use super::{
    backend::BackendStatus,
    runtime::{AgentRuntime, AttemptSummary},
    selection::AgentSelection,
};

#[derive(Debug)]
pub struct AgentService {
    current: RwLock<Arc<AgentRuntime>>,
    parse_gate: Mutex<()>,
    active: StdMutex<Option<Arc<AgentRuntime>>>,
}

impl AgentService {
    pub fn new(selection: AgentSelection) -> Self {
        Self {
            current: RwLock::new(Arc::new(AgentRuntime::for_selection(selection))),
            parse_gate: Mutex::new(()),
            active: StdMutex::new(None),
        }
    }

    pub async fn status(&self) -> BackendStatus {
        self.current.read().await.status().await
    }

    pub async fn probe(&self, database: Arc<Database>) -> BackendStatus {
        let runtime = Arc::clone(&*self.current.read().await);
        let _ = runtime.probe(database).await;
        self.status().await
    }

    pub async fn select(
        &self,
        database: &Database,
        selection: AgentSelection,
    ) -> AppResult<BackendStatus> {
        selection.validate()?;
        database.set_agent_selection(selection.clone())?;
        *self.current.write().await = Arc::new(AgentRuntime::for_selection(selection));
        Ok(self.status().await)
    }

    pub async fn parse_source(
        &self,
        database: Arc<Database>,
        source_id: String,
    ) -> AppResult<AttemptSummary> {
        let _guard = self.parse_gate.lock().await;
        let runtime = Arc::clone(&*self.current.read().await);
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| AppError::storage("解析状态锁已损坏"))?;
            *active = Some(Arc::clone(&runtime));
        }
        let outcome = runtime.parse_source(database, source_id).await;
        if let Ok(mut active) = self.active.lock() {
            active.take();
        }
        outcome
    }

    pub async fn cancel(&self) -> AppResult<bool> {
        let runtime = self
            .active
            .lock()
            .map_err(|_| AppError::storage("解析状态锁已损坏"))?
            .clone();
        match runtime {
            Some(runtime) => runtime.cancel().await,
            None => Ok(false),
        }
    }

    pub async fn shutdown(&self) -> AppResult<bool> {
        self.cancel().await
    }
}
