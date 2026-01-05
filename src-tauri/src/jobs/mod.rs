use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Status of an organize job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is currently running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed with an error
    Failed,
    /// Job was interrupted (app crashed/closed)
    Interrupted,
}

/// A single operation in the organize plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeOperation {
    pub op_id: String,
    #[serde(rename = "type")]
    pub op_type: String,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub path: Option<String>,
    pub new_name: Option<String>,
}

/// The full organize plan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub plan_id: String,
    pub description: String,
    pub operations: Vec<OrganizeOperation>,
    pub target_folder: String,
    /// Whether folder structure simplification is recommended (set when 0 operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simplification_recommended: Option<bool>,
}

impl OrganizePlan {
    /// Compute a hash of the plan for validation.
    /// This is used to ensure the plan hasn't been modified between simulation and execution.
    pub fn compute_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the plan ID
        self.plan_id.hash(&mut hasher);

        // Hash all operations in order
        for op in &self.operations {
            op.op_id.hash(&mut hasher);
            op.op_type.hash(&mut hasher);
            op.source.hash(&mut hasher);
            op.destination.hash(&mut hasher);
            op.path.hash(&mut hasher);
            op.new_name.hash(&mut hasher);
        }

        // Hash target folder
        self.target_folder.hash(&mut hasher);

        format!("{:016x}", hasher.finish())
    }
}

/// Persistent state for an organize job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeJob {
    /// Unique job identifier
    pub job_id: String,
    /// Target folder being organized
    pub target_folder: String,
    /// Human-readable folder name
    pub folder_name: String,
    /// When the job started (unix timestamp ms)
    pub started_at: u64,
    /// Current status
    pub status: JobStatus,
    /// The organization plan
    pub plan: Option<OrganizePlan>,
    /// IDs of operations that have completed
    pub completed_ops: Vec<String>,
    /// Index of current operation being executed
    pub current_op_index: i32,
    /// Last update timestamp (unix timestamp ms)
    pub last_updated_at: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Total operations count
    pub total_ops: usize,
}

impl OrganizeJob {
    /// Create a new job for a folder
    pub fn new(target_folder: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let folder_name = target_folder
            .split('/')
            .last()
            .unwrap_or("folder")
            .to_string();

        Self {
            job_id: format!("job-{}", now),
            target_folder: target_folder.to_string(),
            folder_name,
            started_at: now,
            status: JobStatus::Running,
            plan: None,
            completed_ops: Vec::new(),
            current_op_index: -1,
            last_updated_at: now,
            error: None,
            total_ops: 0,
        }
    }

    /// Update the job with a plan
    pub fn set_plan(&mut self, plan: OrganizePlan) {
        self.total_ops = plan.operations.len();
        self.plan = Some(plan);
        self.update_timestamp();
    }

    /// Mark an operation as completed
    pub fn complete_operation(&mut self, op_id: &str) {
        if !self.completed_ops.contains(&op_id.to_string()) {
            self.completed_ops.push(op_id.to_string());
        }
        self.update_timestamp();
    }

    /// Set current operation index
    pub fn set_current_op(&mut self, index: i32) {
        self.current_op_index = index;
        self.update_timestamp();
    }

    /// Mark job as completed
    pub fn mark_completed(&mut self) {
        self.status = JobStatus::Completed;
        self.current_op_index = -1;
        self.update_timestamp();
    }

    /// Mark job as failed
    pub fn mark_failed(&mut self, error: &str) {
        self.status = JobStatus::Failed;
        self.error = Some(error.to_string());
        self.update_timestamp();
    }

    /// Mark job as interrupted (called on recovery)
    pub fn mark_interrupted(&mut self) {
        if self.status == JobStatus::Running {
            self.status = JobStatus::Interrupted;
            self.update_timestamp();
        }
    }

    fn update_timestamp(&mut self) {
        self.last_updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
    }
}

/// Job persistence manager
pub struct JobManager;

impl JobManager {
    /// Get the path to the job state file
    fn get_job_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("sentinel").join("current_job.json"))
    }

    /// Save the current job state
    pub fn save_job(job: &OrganizeJob) -> Result<(), String> {
        let path = Self::get_job_path().ok_or("Could not determine config directory")?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let json = serde_json::to_string_pretty(job)
            .map_err(|e| format!("Failed to serialize job: {}", e))?;

        fs::write(&path, json).map_err(|e| format!("Failed to write job file: {}", e))?;

        eprintln!("[JobManager] Saved job state: {} (status: {:?})", job.job_id, job.status);
        Ok(())
    }

    /// Load the current job state (if any)
    pub fn load_job() -> Result<Option<OrganizeJob>, String> {
        let path = Self::get_job_path().ok_or("Could not determine config directory")?;

        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read job file: {}", e))?;

        let job: OrganizeJob = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse job file: {}", e))?;

        eprintln!("[JobManager] Loaded job: {} (status: {:?})", job.job_id, job.status);
        Ok(Some(job))
    }

    /// Clear the current job state
    pub fn clear_job() -> Result<(), String> {
        let path = Self::get_job_path().ok_or("Could not determine config directory")?;

        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Failed to delete job file: {}", e))?;
            eprintln!("[JobManager] Cleared job state");
        }

        Ok(())
    }

    /// Check for interrupted jobs on startup
    /// Returns the job if it was running when the app closed
    pub fn check_for_interrupted_job() -> Result<Option<OrganizeJob>, String> {
        if let Some(mut job) = Self::load_job()? {
            if job.status == JobStatus::Running {
                // Job was running when app closed - mark as interrupted
                job.mark_interrupted();
                Self::save_job(&job)?;
                return Ok(Some(job));
            } else if job.status == JobStatus::Interrupted {
                // Already marked as interrupted
                return Ok(Some(job));
            }
            // Completed or failed jobs can be ignored
        }
        Ok(None)
    }
}
