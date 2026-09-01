use kivo_lib::product_build::build_product;
use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let product = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("build"), Some(product), None) => product,
        _ => {
            eprintln!("usage: kivo-product build <product-version-id>");
            return ExitCode::from(2);
        }
    };
    let repo_root = env::var_os("KIVO_REPOSITORY_ROOT")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .and_then(|path| path.canonicalize().ok());
    let Some(repo_root) = repo_root else {
        eprintln!("unable to resolve repository root");
        return ExitCode::FAILURE;
    };
    let build_id = env::var("KIVO_FIRMWARE_BUILD_ID").unwrap_or_else(|_| "dev".into());
    match build_product(&repo_root, &product, &build_id, |line| println!("{line}")) {
        Ok(output) => {
            println!("{}", output.manifest_path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
