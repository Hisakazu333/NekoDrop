#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeFileState {
    pub path: String,
    pub received_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePlan {
    pub transfer_id: String,
    pub files: Vec<ResumeFileState>,
}

impl ResumePlan {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}
