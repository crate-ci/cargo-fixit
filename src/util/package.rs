use cargo_util_schemas::core::PackageIdSpec;

use crate::CargoResult;

pub fn format_package_id(package_id: &str) -> CargoResult<String> {
    let spec = PackageIdSpec::parse(package_id)?;
    let version = spec
        .version()
        .map(|v| v.to_string())
        .unwrap_or("0.0.0".to_owned());

    Ok(format!("{} v{}", spec.name(), version))
}
