//! Dependency fetching and verification.

use std::io::Read;
use std::path::Path;

use base64::prelude::*;
use eos_core::digest::Blake3Digest;
use eos_core::eval::ResolvedInput;
use eos_core::store::StorePath;
use eos_snix::SnixEngine;
use sha2::{Digest, Sha256};

/// Helper to download a file from a URL.
async fn download_file(url: &str, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directory: {}", e))?;
    }
    let output = tokio::process::Command::new("curl")
        .args(["-L", url, "-o", out_path.to_str().unwrap()])
        .output()
        .await
        .map_err(|e| format!("failed to execute curl: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "curl failed to download {}: {}",
            url,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Helper to extract a tarball.
async fn extract_tarball(tar_path: &Path, out_dir: &Path) -> Result<(), String> {
    tokio::fs::create_dir_all(out_dir)
        .await
        .map_err(|e| format!("failed to create extraction directory: {}", e))?;
    let output = tokio::process::Command::new("tar")
        .args([
            "-xf",
            tar_path.to_str().unwrap(),
            "-C",
            out_dir.to_str().unwrap(),
            "--strip-components=1", // Remove top-level directory per spec
        ])
        .output()
        .await
        .map_err(|e| format!("failed to execute tar: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "tar extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

async fn run_git_cmd(out_dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(out_dir)
        .output()
        .await
        .map_err(|e| format!("failed to execute git: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "git command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Helper to clone/fetch a git repository.
async fn fetch_git(url: &str, rev: &str, out_dir: &Path) -> Result<(), String> {
    tokio::fs::create_dir_all(out_dir)
        .await
        .map_err(|e| format!("failed to create git directory: {}", e))?;

    run_git_cmd(out_dir, &["init"]).await?;
    run_git_cmd(out_dir, &["remote", "add", "origin", url]).await?;

    // Try shallow fetch first for performance
    if run_git_cmd(out_dir, &["fetch", "--depth=1", "origin", rev])
        .await
        .is_err()
    {
        // Fallback to full fetch if shallow fetch fails
        run_git_cmd(out_dir, &["fetch", "origin"]).await?;
    }

    run_git_cmd(out_dir, &["checkout", rev]).await?;
    Ok(())
}

/// Verifies file contents against SRI hash format.
fn verify_file_hash(path: &Path, expected_sri: &str) -> Result<(), String> {
    let parts: Vec<&str> = if expected_sri.contains('-') {
        expected_sri.splitn(2, '-').collect()
    } else if expected_sri.contains(':') {
        expected_sri.splitn(2, ':').collect()
    } else {
        return Err(format!("Invalid SRI hash format: {}", expected_sri));
    };

    if parts.len() != 2 {
        return Err(format!("Invalid SRI hash format: {}", expected_sri));
    }

    let algo = parts[0];
    let digest_str = parts[1];

    if algo != "sha256" {
        return Err(format!("Unsupported hash algorithm: {}", algo));
    }

    // Decode expected bytes
    let expected_bytes = if expected_sri.contains('-') {
        BASE64_STANDARD
            .decode(digest_str.trim())
            .map_err(|e| format!("Failed to decode base64 hash: {}", e))?
    } else {
        // Nixbase32 or hex. Try nixbase32 first
        if let Ok(bytes) = nix_compat::nixbase32::decode(digest_str.as_bytes()) {
            bytes
        } else {
            // Try hex
            hex::decode(digest_str)
                .map_err(|e| format!("Failed to decode nixbase32 or hex hash: {}", e))?
        }
    };

    // Compute file hash
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let actual_bytes = hasher.finalize();

    if actual_bytes[..] != expected_bytes[..] {
        return Err(format!(
            "Hash mismatch for {:?}: expected {}, got sha256-{}",
            path,
            expected_sri,
            BASE64_STANDARD.encode(actual_bytes)
        ));
    }

    Ok(())
}

/// Fetches a non-atom dependency directly from URLs, verifies it, imports it,
/// and returns the ResolvedInput.
pub async fn fetch_external(
    desc: &eos_core::request::FetchDescriptor,
    engine: &SnixEngine,
    sandbox_workdir: &Path,
) -> Result<ResolvedInput<Blake3Digest>, String> {
    let name = desc.name();
    let temp_dir = sandbox_workdir.join("fetch-temp").join(name);
    if temp_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    match desc {
        eos_core::request::FetchDescriptor::Atom(_) => {
            Err("Atom dependencies cannot be fetched via fetch_external".to_string())
        },
        eos_core::request::FetchDescriptor::NixGit(nix_git_dep) => {
            let out_path = temp_dir.join("checkout");
            fetch_git(&nix_git_dep.url, &nix_git_dep.rev, &out_path).await?;

            let path_info = snix_store::import::import_path_as_nar_ca(
                &out_path,
                name,
                engine.blob_service.clone(),
                engine.directory_service.clone(),
                &engine.path_info_service,
                &*engine.nar_calculation_service,
            )
            .await
            .map_err(|e| format!("Failed to import nix+git dependency to store: {}", e))?;

            let digest = match &path_info.node {
                snix_castore::Node::File { digest, .. } => *digest,
                snix_castore::Node::Directory { digest, .. } => *digest,
                snix_castore::Node::Symlink { .. } => {
                    return Err("Nix+git dependency cannot be a symlink node".to_string());
                },
            };

            Ok(ResolvedInput {
                digest: Blake3Digest(digest.into()),
                store_path: StorePath(path_info.store_path.to_string()),
            })
        },
        eos_core::request::FetchDescriptor::Nix(nix_dep) => {
            let file_path = temp_dir.join(&nix_dep.name);
            download_file(&nix_dep.url, &file_path).await?;
            verify_file_hash(&file_path, &nix_dep.hash)?;

            let path_info = snix_store::import::import_path_as_nar_ca(
                &file_path,
                name,
                engine.blob_service.clone(),
                engine.directory_service.clone(),
                &engine.path_info_service,
                &*engine.nar_calculation_service,
            )
            .await
            .map_err(|e| format!("Failed to import nix file to store: {}", e))?;

            let digest = match &path_info.node {
                snix_castore::Node::File { digest, .. } => *digest,
                snix_castore::Node::Directory { digest, .. } => *digest,
                snix_castore::Node::Symlink { .. } => {
                    return Err("Nix file cannot be a symlink node".to_string());
                },
            };

            Ok(ResolvedInput {
                digest: Blake3Digest(digest.into()),
                store_path: StorePath(path_info.store_path.to_string()),
            })
        },
        eos_core::request::FetchDescriptor::NixTar(nix_tar_dep) => {
            let tar_path = temp_dir.join("archive.tar.gz");
            download_file(&nix_tar_dep.url, &tar_path).await?;
            verify_file_hash(&tar_path, &nix_tar_dep.hash)?;

            let out_dir = temp_dir.join("extracted");
            extract_tarball(&tar_path, &out_dir).await?;

            let path_info = snix_store::import::import_path_as_nar_ca(
                &out_dir,
                name,
                engine.blob_service.clone(),
                engine.directory_service.clone(),
                &engine.path_info_service,
                &*engine.nar_calculation_service,
            )
            .await
            .map_err(|e| format!("Failed to import nix+tar dependency to store: {}", e))?;

            let digest = match &path_info.node {
                snix_castore::Node::File { digest, .. } => *digest,
                snix_castore::Node::Directory { digest, .. } => *digest,
                snix_castore::Node::Symlink { .. } => {
                    return Err("Nix+tar dependency cannot be a symlink node".to_string());
                },
            };

            Ok(ResolvedInput {
                digest: Blake3Digest(digest.into()),
                store_path: StorePath(path_info.store_path.to_string()),
            })
        },
        eos_core::request::FetchDescriptor::NixSrc(nix_src_dep) => {
            let file_path = temp_dir.join(&nix_src_dep.name);
            download_file(&nix_src_dep.url, &file_path).await?;
            verify_file_hash(&file_path, &nix_src_dep.hash)?;

            let path_info = snix_store::import::import_path_as_nar_ca(
                &file_path,
                name,
                engine.blob_service.clone(),
                engine.directory_service.clone(),
                &engine.path_info_service,
                &*engine.nar_calculation_service,
            )
            .await
            .map_err(|e| format!("Failed to import nix+src dependency to store: {}", e))?;

            let digest = match &path_info.node {
                snix_castore::Node::File { digest, .. } => *digest,
                snix_castore::Node::Directory { digest, .. } => *digest,
                snix_castore::Node::Symlink { .. } => {
                    return Err("Nix+src dependency cannot be a symlink node".to_string());
                },
            };

            Ok(ResolvedInput {
                digest: Blake3Digest(digest.into()),
                store_path: StorePath(path_info.store_path.to_string()),
            })
        },
    }
}

/// Resolves an atom dependency via AtomSource, checks out its content, and imports it.
pub async fn fetch_atom<S: atom_core::AtomSource>(
    desc: &eos_core::request::AtomFetchDescriptor,
    source: &S,
    engine: &SnixEngine,
    sandbox_workdir: &Path,
) -> Result<ResolvedInput<Blake3Digest>, String> {
    use atom_core::{AtomEntry, AtomVersion};
    let name = &desc.label;

    let entry_opt = source
        .resolve(&desc.id)
        .await
        .map_err(|e| format!("Failed to resolve atom {}: {}", name, e))?;

    let entry = entry_opt.ok_or_else(|| format!("Atom {} not found in source", name))?;

    let version_entry = entry
        .versions()
        .find(|v| v.version().as_str() == desc.version.as_str())
        .ok_or_else(|| format!("Version {} not found for atom {}", desc.version, name))?;

    // Downcast to retrieve the underlying Git repository path
    let repo_path = if let Some(git_source) = source.as_any().downcast_ref::<atom_git::GitSource>()
    {
        git_source.repo().path().to_path_buf()
    } else if let Some(git_store) = source.as_any().downcast_ref::<atom_git::GitStore>() {
        git_store.source.repo().path().to_path_buf()
    } else if let Some(git_registry) = source.as_any().downcast_ref::<atom_git::GitRegistry>() {
        git_registry.source.repo().path().to_path_buf()
    } else {
        return Err(format!(
            "Unsupported AtomSource type for atom {} (must be git-backed)",
            name
        ));
    };

    let temp_dir = sandbox_workdir.join("fetch-temp").join(name);
    if temp_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
    let out_path = temp_dir.join("checkout");
    tokio::fs::create_dir_all(&out_path)
        .await
        .map_err(|e| format!("Failed to create checkout directory: {}", e))?;

    let hex_oid = hex::encode(version_entry.dig());
    let tar_file_path = temp_dir.join("atom.tar");

    let git_archive_output = tokio::process::Command::new("git")
        .args([
            "--git-dir",
            repo_path.to_str().unwrap(),
            "archive",
            "--format=tar",
            "-o",
            tar_file_path.to_str().unwrap(),
            &hex_oid,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to execute git archive: {}", e))?;

    if !git_archive_output.status.success() {
        return Err(format!(
            "git archive failed for OID {}: {}",
            hex_oid,
            String::from_utf8_lossy(&git_archive_output.stderr)
        ));
    }

    let tar_extract_output = tokio::process::Command::new("tar")
        .args([
            "-xf",
            tar_file_path.to_str().unwrap(),
            "-C",
            out_path.to_str().unwrap(),
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to execute tar extraction: {}", e))?;

    if !tar_extract_output.status.success() {
        return Err(format!(
            "tar extraction failed: {}",
            String::from_utf8_lossy(&tar_extract_output.stderr)
        ));
    }

    let _ = tokio::fs::remove_file(&tar_file_path).await;

    // Import into SnixStore
    let path_info = snix_store::import::import_path_as_nar_ca(
        &out_path,
        name,
        engine.blob_service.clone(),
        engine.directory_service.clone(),
        &engine.path_info_service,
        &*engine.nar_calculation_service,
    )
    .await
    .map_err(|e| format!("Failed to import local atom to store: {}", e))?;

    let digest = match &path_info.node {
        snix_castore::Node::File { digest, .. } => *digest,
        snix_castore::Node::Directory { digest, .. } => *digest,
        snix_castore::Node::Symlink { .. } => {
            return Err("Atom source cannot be a symlink node".to_string());
        },
    };

    Ok(ResolvedInput {
        digest: Blake3Digest(digest.into()),
        store_path: StorePath(path_info.store_path.to_string()),
    })
}
