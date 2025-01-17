use crate::shared::{
    self,
    dialogs,
};
use anyhow::{bail, Result};
use std::{fs, path::PathBuf, process::Command};
use velopack::{bundle, locator::VelopackLocator};

pub fn apply_package_impl<'a>(locator: &VelopackLocator, pkg: &PathBuf, _runhooks: bool) -> Result<VelopackLocator> {
    let root_path = locator.get_root_dir();
    let tmp_path_new = locator.get_temp_dir_rand16();
    let tmp_path_old = locator.get_temp_dir_rand16();
    let mut bundle = bundle::load_bundle_from_file(pkg)?;
    let manifest = bundle.read_manifest()?;
    let new_locator = locator.clone_self_with_new_manifest(&manifest);

    let action: Result<()> = (|| {
        // 1. extract the bundle to a temp dir
        fs::create_dir_all(&tmp_path_new)?;
        info!("Extracting bundle to {:?}", &tmp_path_new);
        bundle.extract_lib_contents_to_path(&tmp_path_new, |_| {})?;

        // 2. attempt to replace the current bundle with the new one
        let result: Result<()> = (|| {
            info!("Replacing bundle at {:?}", &root_path);
            fs::rename(&root_path, &tmp_path_old)?;
            fs::rename(&tmp_path_new, &root_path)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                info!("Bundle extracted successfully to {:?}", &root_path);
                Ok(())
            }
            Err(e) => {
                // 3. if fails for permission error, try again escalated via osascript
                if shared::is_error_permission_denied(&e) {
                    error!("A permissions error occurred ({}), will attempt to elevate permissions and try again...", e);
                    dialogs::ask_user_to_elevate(&manifest.title, &manifest.version.to_string())?;
                    let script = format!(
                        "do shell script \"mv -f '{}' '{}' && mv -f '{}' '{}' && rm -rf '{}'\" with administrator privileges",
                        &root_path.to_string_lossy(),
                        &tmp_path_old.to_string_lossy(),
                        &tmp_path_new.to_string_lossy(),
                        &root_path.to_string_lossy(),
                        &tmp_path_old.to_string_lossy()
                    );
                    info!("Running elevated process via osascript: {}", script);
                    let output = Command::new("osascript").arg("-e").arg(&script).status()?;
                    if output.success() {
                        info!("Bundle applied successfully via osascript.");
                        Ok(())
                    } else {
                        bail!("elevated process failed: exited with code: {}", output);
                    }
                } else {
                    bail!("Failed to extract bundle ({})", e);
                }
            }
        }
    })();
    let _ = fs::remove_dir_all(&tmp_path_new);
    let _ = fs::remove_dir_all(&tmp_path_old);
    action?;
    Ok(new_locator)
}
