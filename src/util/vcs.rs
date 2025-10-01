use std::env;

use anyhow::bail;
use clap::Parser;

use crate::CargoResult;

#[derive(Parser, Debug)]
pub struct VcsOpts {
    /// Fix code even if a VCS was not detected
    #[arg(long)]
    pub allow_no_vcs: bool,

    /// Fix code even if the working directory is dirty or has staged changes
    #[arg(long)]
    pub allow_dirty: bool,

    /// Fix code even if the working directory has staged changes
    #[arg(long)]
    pub allow_staged: bool,
}

impl VcsOpts {
    pub fn valid_vcs(&self) -> CargoResult<()> {
        if self.allow_no_vcs {
            return Ok(());
        }
        let cwd = env::current_dir()?;

        let repo = git2::Repository::discover(&cwd).ok().filter(|r| {
            if r.workdir().is_some_and(|workdir| workdir == cwd) {
                true
            } else {
                !r.is_path_ignored(cwd).unwrap_or(false)
            }
        });

        let Some(repo) = repo else {
            bail!(
                "no VCS found for this package and `cargo fix` can potentially \
                perform destructive changes; if you'd like to suppress this \
                error pass `--allow-no-vcs`"
            );
        };

        if self.allow_staged && self.allow_dirty {
            return Ok(());
        }
        let mut dirty_files = Vec::new();
        let mut staged_files = Vec::new();

        let mut repo_opts = git2::StatusOptions::new();
        repo_opts.include_ignored(false);
        repo_opts.include_untracked(true);
        for status in repo.statuses(Some(&mut repo_opts))?.iter() {
            if let Some(path) = status.path() {
                match status.status() {
                    git2::Status::CURRENT => (),
                    git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::INDEX_RENAMED
                    | git2::Status::INDEX_TYPECHANGE => {
                        if !self.allow_staged {
                            staged_files.push(path.to_owned());
                        }
                    }
                    _ => {
                        if !self.allow_dirty {
                            dirty_files.push(path.to_owned());
                        }
                    }
                };
            }
        }

        if dirty_files.is_empty() && staged_files.is_empty() {
            return Ok(());
        }

        let mut files_list = String::new();
        for file in dirty_files {
            files_list.push_str("  * ");
            files_list.push_str(&file);
            files_list.push_str(" (dirty)\n");
        }
        for file in staged_files {
            files_list.push_str("  * ");
            files_list.push_str(&file);
            files_list.push_str(" (staged)\n");
        }

        bail!(
            "the working directory of this package has uncommitted changes, and \
            `cargo fix` can potentially perform destructive changes; if you'd \
            like to suppress this error pass `--allow-dirty`, \
            or commit the changes to these files:\n\
            \n\
            {files_list}\n\
            "
        );
    }
}
