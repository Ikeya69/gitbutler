use std::{collections::HashMap, path, sync::Arc};

use anyhow::Context;
use tauri::AppHandle;
use tokio::sync::Semaphore;

use crate::{
    gb_repository,
    project_repository::{self, conflicts},
    projects, users,
};

pub struct Controller {
    local_data_dir: path::PathBuf,
    semaphores: Arc<tokio::sync::Mutex<HashMap<String, Semaphore>>>,

    projects_storage: projects::Storage,
    users_storage: users::Storage,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project is in a conflicted state")]
    Conflicting,
    #[error(transparent)]
    LockError(#[from] tokio::sync::AcquireError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl TryFrom<&AppHandle> for Controller {
    type Error = Error;

    fn try_from(value: &AppHandle) -> Result<Self, Self::Error> {
        let local_data_dir = value
            .path_resolver()
            .app_local_data_dir()
            .context("Failed to get local data dir")?;
        Ok(Self {
            local_data_dir,
            semaphores: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            projects_storage: projects::Storage::from(value),
            users_storage: users::Storage::from(value),
        })
    }
}

impl Controller {
    pub async fn create_commit(
        &self,
        project_id: &str,
        branch: &str,
        message: &str,
    ) -> Result<(), Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        super::commit(&gb_repository, &project_repository, branch, message)?;

        Ok(())
    }

    pub async fn list_virtual_branches(
        &self,
        project_id: &str,
    ) -> Result<Vec<super::VirtualBranch>, Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        let branches = super::list_virtual_branches(&gb_repository, &project_repository)?;
        Ok(branches)
    }

    pub async fn create_virtual_branch(
        &self,
        project_id: &str,
        create: &super::branch::BranchCreateRequest,
    ) -> Result<(), Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        if conflicts::is_resolving(&project_repository) {
            return Err(Error::Conflicting);
        }

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        super::create_virtual_branch(&gb_repository, create)?;
        Ok(())
    }

    pub async fn create_virtual_branch_from_branch(
        &self,
        project_id: &str,
        branch: &project_repository::branch::Name,
    ) -> Result<String, Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        let branch_id = super::create_virtual_branch_from_branch(
            &gb_repository,
            &project_repository,
            branch,
            None,
        )?;

        // also apply the branch
        super::apply_branch(&gb_repository, &project_repository, &branch_id)?;

        Ok(branch_id)
    }

    pub fn get_base_branch_data(
        &self,
        project_id: &str,
    ) -> Result<Option<super::BaseBranch>, Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let base_branch = super::get_base_branch_data(&gb_repository, &project_repository)?;
        Ok(base_branch)
    }

    pub async fn set_base_branch(
        &self,
        project_id: &str,
        target_branch: &str,
    ) -> Result<super::BaseBranch, Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        let target = super::set_base_branch(&gb_repository, &project_repository, target_branch)?;
        Ok(target)
    }

    pub async fn update_base_branch(&self, project_id: &str) -> Result<(), Error> {
        let project = self
            .projects_storage
            .get_project(project_id)
            .context("failed to get project")?
            .context("project not found")?;
        let project_repository = project
            .as_ref()
            .try_into()
            .context("failed to open project repository")?;
        let gb_repository = self.open_gb_repository(project_id)?;

        let mut semaphores = self.semaphores.lock().await;
        let semaphore = semaphores
            .entry(project_id.to_string())
            .or_insert_with(|| Semaphore::new(1));
        let _permit = semaphore.acquire().await?;

        super::update_base_branch(&gb_repository, &project_repository)?;
        Ok(())
    }

    fn open_gb_repository(&self, project_id: &str) -> Result<gb_repository::Repository, Error> {
        gb_repository::Repository::open(
            self.local_data_dir.clone(),
            project_id,
            self.projects_storage.clone(),
            self.users_storage.clone(),
        )
        .context("failed to open repository")
        .map_err(Error::Other)
    }
}