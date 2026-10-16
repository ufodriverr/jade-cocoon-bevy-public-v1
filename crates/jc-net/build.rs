//! The name of the build that the fight of two players says in its hello (`BUILD` of
//! `src/lib.rs`): the version and the commit the source was built from, so that two PCs
//! with builds of different commits (of the same version) refuse each other
//! with the reason, rather than starting a fight whose rules may differ and stopping it
//! later with a desync. A debug and a release build of the same commit have the same
//! name. A release's complete source archive names its commit in `SOURCE-REVISION.txt`,
//! which is read first, so a build from it plays with the released executable. Without
//! either (a copy of the source without its history) the name is the version alone.

use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = Path::new(&manifest).join("../..");
    // Built anew when the commit changes: HEAD, the branch it names and the packed refs.
    let git = root.join(".git");
    let head = git.join("HEAD");
    if head.is_file() {
        println!("cargo:rerun-if-changed={}", head.display());
        let named = std::fs::read_to_string(&head).unwrap_or_default();
        if let Some(branch) = named.trim().strip_prefix("ref: ") {
            for file in [git.join(branch), git.join("packed-refs")] {
                if file.is_file() {
                    println!("cargo:rerun-if-changed={}", file.display());
                }
            }
        }
    }
    // The commit a release's source archive names, if this is one.
    let revision = root.join("SOURCE-REVISION.txt");
    if revision.is_file() {
        println!("cargo:rerun-if-changed={}", revision.display());
    }
    let released = std::fs::read_to_string(&revision)
        .ok()
        .map(|text| text.trim().chars().take(10).collect::<String>())
        .filter(|commit| commit.len() == 10 && commit.chars().all(|c| c.is_ascii_hexdigit()));
    let commit = released.or_else(|| {
        Command::new("git")
            .args(["rev-parse", "--short=10", "HEAD"])
            .current_dir(&root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|commit| commit.trim().to_string())
            .filter(|commit| !commit.is_empty() && commit.chars().all(|c| c.is_ascii_hexdigit()))
    });
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let build = match commit {
        Some(commit) => format!("{version}+{commit}"),
        None => version,
    };
    println!("cargo:rustc-env=JC_BUILD={build}");
}
