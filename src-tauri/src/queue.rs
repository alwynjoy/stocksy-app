use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    AlreadyExists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedJob {
    pub id: String,
    pub ticker: String,
    pub country_code: String,
    pub provider: String,
    pub model: String,
    pub company_name: Option<String>,
    pub status: JobStatus,
    pub enqueued_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub report_id: Option<String>,
    pub message: Option<String>,
    pub thinking: String,
    pub steps: Vec<StepState>,
    pub debug_log_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub id: String,
    pub label: String,
    pub status: String, // pending|running|completed|failed
    pub message: Option<String>,
}

fn pipeline_labels() -> Vec<String> {
    vec![
        "Fetching Data".into(),
        "Validating".into(),
        "AI Analysis".into(),
        "Saving".into(),
    ]
}

fn initial_steps() -> Vec<StepState> {
    pipeline_labels()
        .into_iter()
        .enumerate()
        .map(|(i, l)| StepState {
            id: i.to_string(),
            label: l,
            status: if i == 0 { "running".into() } else { "pending".into() },
            message: None,
        })
        .collect()
}

pub struct QueueState {
    pub pending: VecDeque<QueuedJob>,
    pub active: HashMap<String, QueuedJob>,
    pub max_parallel: usize,
    // completed jobs kept briefly for UI (in-memory only, not persisted)
    pub recent: VecDeque<QueuedJob>,
}

impl QueueState {
    pub fn new(max_parallel: usize) -> Self {
        Self { pending: VecDeque::new(), active: HashMap::new(), max_parallel: max_parallel.clamp(1,5), recent: VecDeque::new() }
    }
}

pub struct QueueHandle(pub Arc<Mutex<QueueState>>);

impl QueueHandle {
    pub fn new(max_parallel: usize) -> Self {
        Self(Arc::new(Mutex::new(QueueState::new(max_parallel))))
    }
}

fn emit_queue_update(app: &AppHandle, state: &QueueState) {
    let pending: Vec<QueuedJob> = state.pending.iter().cloned().collect();
    let active: Vec<QueuedJob> = state.active.values().cloned().collect();
    let recent: Vec<QueuedJob> = state.recent.iter().cloned().collect();
    let _ = app.emit("queue://update", serde_json::json!({"pending": pending, "active": active, "recent": recent, "max_parallel": state.max_parallel}));
}

pub fn set_job_step(app: &AppHandle, job_id: &str, idx: usize, status: &str, message: Option<String>) {
    if let Some(handle) = app.try_state::<QueueHandle>() {
        let mut s = handle.0.lock().unwrap();
        let mut found = false;
        for job in s.active.values_mut() {
            if job.id == job_id {
                if let Some(step) = job.steps.get_mut(idx) {
                    step.status = status.to_string();
                    step.message = message.clone();
                }
                if status == "failed" {
                    job.status = JobStatus::Failed;
                }
                found = true;
                break;
            }
        }
        if !found {
            for job in s.pending.iter_mut() {
                if job.id == job_id {
                    if let Some(step) = job.steps.get_mut(idx) {
                        step.status = status.to_string();
                        step.message = message.clone();
                    }
                    if status == "failed" {
                        job.status = JobStatus::Failed;
                    }
                    break;
                }
            }
        }
        // Also check recent for completeness (in case job already moved)
        for job in s.recent.iter_mut() {
            if job.id == job_id {
                if let Some(step) = job.steps.get_mut(idx) {
                    step.status = status.to_string();
                    step.message = message.clone();
                }
                break;
            }
        }
    }
}

pub fn append_job_thinking(app: &AppHandle, job_id: &str, delta: &str) {
    if delta.is_empty() { return; }
    if let Some(handle) = app.try_state::<QueueHandle>() {
        let mut s = handle.0.lock().unwrap();
        let mut found = false;
        for job in s.active.values_mut() {
            if job.id == job_id {
                job.thinking.push_str(delta);
                found = true;
                break;
            }
        }
        if !found {
            for job in s.pending.iter_mut() {
                if job.id == job_id {
                    job.thinking.push_str(delta);
                    break;
                }
            }
        }
    }
}

pub fn set_job_debug_log(app: &AppHandle, job_id: &str, log_id: String) {
    if let Some(handle) = app.try_state::<QueueHandle>() {
        let mut s = handle.0.lock().unwrap();
        let mut found = false;
        for job in s.active.values_mut() {
            if job.id == job_id {
                job.debug_log_id = Some(log_id.clone());
                found = true;
                break;
            }
        }
        if !found {
            for job in s.pending.iter_mut() {
                if job.id == job_id {
                    job.debug_log_id = Some(log_id);
                    break;
                }
            }
        }
    }
}

pub async fn enqueue(
    app: AppHandle,
    db_path: PathBuf,
    queue: Arc<Mutex<QueueState>>,
    ticker: String,
    country_code: String,
    provider: String,
) -> Result<String, String> {
    // Resolve model for display
    let model = {
        let conn = crate::db::open(&db_path).map_err(|e| e.to_string())?;
        crate::db::get_setting(&conn, &format!("llm.{}.model", provider.to_lowercase())).ok().flatten().unwrap_or_default()
    };
    let id = format!("job-{}-{}-{}", ticker.to_lowercase(), model.to_lowercase().replace('.' , "-").replace('/' , "-"), chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let job = QueuedJob {
        id: id.clone(),
        ticker: ticker.clone(),
        country_code: country_code.clone(),
        provider: provider.clone(),
        model: model.clone(),
        company_name: None,
        status: JobStatus::Queued,
        enqueued_at: now,
        started_at: None,
        finished_at: None,
        report_id: None,
        message: None,
        thinking: String::new(),
        steps: initial_steps(),
        debug_log_id: None,
    };
    {
        let mut s = queue.lock().unwrap();
        s.pending.push_back(job);
        emit_queue_update(&app, &s);
    }
    // Try to dispatch immediately if slot free
    try_dispatch(app.clone(), db_path.clone(), queue.clone());
    Ok(id)
}

pub fn try_dispatch(app: AppHandle, db_path: PathBuf, queue: Arc<Mutex<QueueState>>) {
    loop {
        let job_opt = {
            let mut s = queue.lock().unwrap();
            if s.active.len() >= s.max_parallel {
                None
            } else {
                s.pending.pop_front()
            }
        };
        let mut job = match job_opt {
            Some(j) => j,
            None => {
                // No pending or at capacity — no state change, no emit to avoid stale overwrite
                break;
            }
        };
        // Mark running
        job.status = JobStatus::Running;
        job.started_at = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        job.steps = initial_steps();
        {
            let mut s = queue.lock().unwrap();
            s.active.insert(job.id.clone(), job.clone());
            emit_queue_update(&app, &s);
        }
        let app2 = app.clone();
        let db2 = db_path.clone();
        let queue2 = queue.clone();
        let job_id = job.id.clone();
        tokio::spawn(async move {
            // Run the actual pipeline with jobId for per-job emits
            let result = crate::pipeline::run_analysis_with_job(db2.clone(), Some(app2.clone()), job.ticker.clone(), job.country_code.clone(), Some(job.provider.clone()), Some(job_id.clone())).await;
            {
                let mut s = queue2.lock().unwrap();
                if let Some(active_job) = s.active.get_mut(&job_id) {
                    match result {
                        Ok(r) => {
                            let short_msg = crate::pipeline::shorten_error(&r.message);
                            active_job.report_id = Some(r.report_id.clone());
                            active_job.message = Some(short_msg.clone());
                            match r.status.as_str() {
                                "completed" | "success" => active_job.status = JobStatus::Completed,
                                "already_exists" => active_job.status = JobStatus::AlreadyExists,
                                "failure" | "error_llm_failed" => active_job.status = JobStatus::Failed,
                                _ => active_job.status = if r.status.contains("fail") { JobStatus::Failed } else { JobStatus::Completed },
                            }
                            // Update steps to completed/failed based on status
                            if active_job.status == JobStatus::Completed || active_job.status == JobStatus::AlreadyExists {
                                for st in &mut active_job.steps { st.status = "completed".into(); }
                            } else {
                                // mark running step as failed — concise code only
                                if let Some(idx) = active_job.steps.iter().position(|x| x.status == "running") {
                                    active_job.steps[idx].status = "failed".into();
                                    active_job.steps[idx].message = Some(short_msg);
                                }
                            }
                        }
                        Err(e) => {
                            let short = crate::pipeline::shorten_error(&e);
                            active_job.status = JobStatus::Failed;
                            active_job.message = Some(short.clone());
                            if let Some(idx) = active_job.steps.iter().position(|x| x.status == "running") {
                                active_job.steps[idx].status = "failed".into();
                                active_job.steps[idx].message = Some(short);
                            }
                        }
                    }
                    active_job.finished_at = Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
                    let finished = active_job.clone();
                    s.active.remove(&job_id);
                    // Keep in recent for UI (max 20, auto-pruned)
                    s.recent.push_back(finished);
                    while s.recent.len() > 20 { s.recent.pop_front(); }
                    emit_queue_update(&app2, &s);
                }
            }
            // Dispatch next pending (sync, no await needed)
            try_dispatch(app2, db2, queue2);
        });
    }
}
