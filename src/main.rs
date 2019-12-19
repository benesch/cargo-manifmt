// Copyright 2019 Nikhil Benesch.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::{self, Write as FmtWrite};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use cargo::core::{Dependency, GitReference, Manifest, Target, Workspace};
use cargo::core::manifest::TargetKind;
use cargo::util::important_paths;
use cargo::util::config::Config;
use semver::VersionReq;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {}", err);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cwd = env::current_dir()?;
    let root = important_paths::find_root_manifest_for_wd(&cwd)?;
    let config = Config::default()?;
    let workspace = Workspace::new(&root, &config)?;
    for package in workspace.members() {
        let manifest = package.manifest();
        let mut out: Vec<u8> = vec![];
        render_manifest(&mut out, package.root(), manifest)?;
        fs::write(package.manifest_path(), out)?;
    }
    Ok(())
}

fn render_manifest<W>(w: &mut W, base: &Path, manifest: &Manifest) -> io::Result<()>
where
    W: io::Write,
{
    let metadata = manifest.metadata();

    writeln!(w, "[package]")?;
    writeln!(w, "name = {}", TomlStr(&manifest.name()))?;
    if let Some(description) = &metadata.description {
        writeln!(w, "description = {}", TomlStr(description))?;
    }
    writeln!(w, "version = {}", TomlStr(&manifest.version().to_string()))?;
    if !metadata.authors.is_empty() {
        writeln!(w, "authors = {}", TomlPrettyArray(&metadata.authors))?;
    }
    if !metadata.keywords.is_empty() {
        writeln!(w, "keywords = {}", TomlPrettyArray(&metadata.keywords))?;
    }
    if !metadata.categories.is_empty() {
        writeln!(w, "categories = {}", TomlPrettyArray(&metadata.categories))?;
    }
    if let Some(license) = &metadata.license {
        writeln!(w, "license = {}", TomlStr(license))?;
    }
    if let Some(license_file) = &metadata.license_file {
        writeln!(w, "license-file = {}", TomlStr(license_file))?;
    }
    if let Some(readme) = &metadata.readme {
        writeln!(w, "readme = {}", TomlStr(readme))?;
    }
    if let Some(homepage) = &metadata.homepage {
        writeln!(w, "homepage = {}", TomlStr(homepage))?;
    }
    if let Some(repository) = &metadata.repository {
        writeln!(w, "repository = {}", TomlStr(repository))?;
    }
    if let Some(documentation) = &metadata.documentation {
        writeln!(w, "documentation = {}", TomlStr(documentation))?;
    }
    if !manifest.exclude().is_empty() {
        writeln!(w, "exclude = {}", TomlPrettyArray(manifest.exclude()))?;
    }
    if !manifest.include().is_empty() {
        writeln!(w, "include = {}", TomlPrettyArray(manifest.include()))?;
    }
    if let Some(links) = manifest.links() {
        writeln!(w, "links = {}", TomlStr(links))?;
    }
    writeln!(w, "edition = {}", TomlStr(&manifest.edition().to_string()))?;
    if let Some(publish) = manifest.publish() {
        if publish.is_empty() {
            writeln!(w, "publish = false")?;
        } else {
            writeln!(w, "publish = {}", TomlPrettyArray(publish))?;
        }
    }
    if let Some(default_run) = manifest.default_run() {
        writeln!(w, "default-run = {}", TomlStr(default_run))?;
    }

    if let Some(custom_metadata) = manifest.custom_metadata() {
        writeln!(w, "\n[package.metadata]")?;
        write!(w, "{}", custom_metadata)?;
    }

    let mut lib = None;
    let mut bins = vec![];
    let mut examples = vec![];
    let mut tests = vec![];
    let mut benches = vec![];
    for target in manifest.targets() {
        match target.kind() {
            TargetKind::Lib(_) => lib = Some(target),
            TargetKind::Bin => bins.push(target),
            TargetKind::Test => tests.push(target),
            TargetKind::Bench => benches.push(target),
            TargetKind::ExampleLib(_) => examples.push(target),
            TargetKind::ExampleBin => examples.push(target),
            TargetKind::CustomBuild => (),
        }
    }

    if let Some(lib) = lib {
        render_target(w, base, &manifest.name(), lib)?;
    }

    for bin in bins {
        render_target(w, base, &manifest.name(), bin)?;
    }

    for example in examples {
        render_target(w, base, &manifest.name(), example)?;
    }

    for test in tests {
        render_target(w, base, &manifest.name(), test)?;
    }

    for bench in benches {
        render_target(w, base, &manifest.name(), bench)?;
    }

    if !manifest.summary().features().is_empty() {
        writeln!(w, "\n[features]")?;
        for (name, specs) in manifest.summary().features() {
            let value: Vec<_> = specs.iter().map(|s| s.to_string(manifest.summary())).collect();
            writeln!(w, "{} = {}", name, TomlFlatArray(&value))?;
        }
    }

    let mut deps = vec![];
    let mut dev_deps = vec![];
    let mut build_deps = vec![];
    for dep in manifest.dependencies() {
        use cargo::core::dependency::Kind;
        match dep.kind() {
            Kind::Normal => deps.push(dep),
            Kind::Development => dev_deps.push(dep),
            Kind::Build => build_deps.push(dep),
        }
    }
    deps.sort_by_key(|dep| dep.name_in_toml());
    dev_deps.sort_by_key(|dep| dep.name_in_toml());
    build_deps.sort_by_key(|dep| dep.name_in_toml());

    if !deps.is_empty() {
        writeln!(w, "\n[dependencies]")?;
        for dep in deps {
            render_dependency(w, base, dep)?;
        }
    }

    if !dev_deps.is_empty() {
        writeln!(w, "\n[dev-dependencies]")?;
        for dep in dev_deps {
            render_dependency(w, base, dep)?;
        }
    }

    if !build_deps.is_empty() {
        writeln!(w, "\n[build-dependencies]")?;
        for dep in build_deps {
            render_dependency(w, base, dep)?;
        }
    }

    Ok(())
}

fn render_target<W>(w: &mut W, base: &Path, package_name: &str, target: &Target) -> io::Result<()>
where
    W: io::Write,
{
    let mut buf = Vec::new();
    let path = rel_path(base, target.src_path().path().unwrap());
    let at_std_path = match target.kind() {
        TargetKind::Lib(_) => path == "src/lib.rs",
        TargetKind::Bin => path == "src/main.rs" || path == format!("src/bin/{}.rs", target.name()),
        TargetKind::Test => path == format!("tests/{}/main.rs", target.name()) || path == format!("tests/{}.rs", target.name()),
        TargetKind::Bench => path == format!("benches/{}/main.rs", target.name()) || path == format!("benches/{}.rs", target.name()),
        TargetKind::ExampleLib(_) | TargetKind::ExampleBin => path == format!("examples/{}/main.rs", target.name()) || path == format!("examples/{}.rs", target.name()),
        _ => false,
    };
    let path_name = {
        let path = Path::new(&path);
        if path.file_stem().unwrap() != "main" {
            path.file_stem().unwrap_or(OsStr::new(""))
        } else {
            path.parent().unwrap_or(Path::new("")).file_name().unwrap_or(OsStr::new(""))
        }
    }.to_string_lossy().into_owned();
    let inferred_name = match target.kind() {
        TargetKind::Lib(_) => package_name,
        TargetKind::Bin if path == "src/main.rs" => package_name,
        TargetKind::Test | TargetKind::ExampleLib(_) | TargetKind::ExampleBin if at_std_path => &path_name,
        _ => "",
    };
    if target.name() != inferred_name {
        writeln!(buf, "name = {}", TomlStr(target.name()))?;
    }
    if !at_std_path {
        writeln!(buf, "path = {}", TomlStr(path))?;
    }
    if !target.harness() {
        writeln!(buf, "harness = false")?;
    }
    if !target.documented() && target.is_lib() {
        writeln!(buf, "doc = false")?;
    }
    if !buf.is_empty() {
        writeln!(w, "\n[{}]", match target.kind() {
            TargetKind::Lib(_) => "lib",
            TargetKind::Bin => "[bin]",
            TargetKind::Test => "[test]",
            TargetKind::Bench => "[bench]",
            TargetKind::ExampleLib(_) | TargetKind::ExampleBin => "[example]",
            TargetKind::CustomBuild => unreachable!(),
        })?;
        w.write(&buf)?;
    }
    Ok(())
}

fn render_dependency<W>(w: &mut W, base: &Path, dep: &Dependency) -> io::Result<()>
where
    W: io::Write,
{
    write!(w, "{} = ", dep.name_in_toml())?;
    let mut meta: Vec<(&'static str, Box<dyn fmt::Display>)> = vec![];
    if dep.package_name() != dep.name_in_toml() {
        meta.push(("package", Box::new(dep.package_name())));
    }
    let source_id = dep.source_id();
    if source_id.is_path() {
        let url = source_id.url();
        meta.push(("path", Box::new(TomlStr(rel_path(base, url.path())))));
    } else if let Some(git_ref) = source_id.git_reference() {
        meta.push(("git", Box::new(TomlStr(source_id.url().clone()))));
        match git_ref {
            GitReference::Tag(tag) => meta.push(("tag", Box::new(TomlStr(tag)))),
            GitReference::Branch(branch) if branch != "master" => meta.push(("branch", Box::new(TomlStr(branch)))),
            GitReference::Rev(rev) => meta.push(("rev", Box::new(TomlStr(rev)))),
            _ => (),
        }
    }
    if !dep.uses_default_features() {
        meta.push(("default-features", Box::new("false")));
    }
    if !dep.features().is_empty() {
        meta.push(("features", Box::new(TomlFlatArray(dep.features()))));
    }
    if dep.is_optional() {
        meta.push(("optional", Box::new("false")));
    }
    if meta.is_empty() {
        write!(w, "{}\n", TomlVersion(dep.version_req()))?;
    } else {
        if dep.version_req().to_string() != "*" {
            meta.insert(0, ("version", Box::new(TomlVersion(dep.version_req()))));
        }
        write!(w, "{{ {} }}\n", meta.iter().map(|(k, v)| format!("{} = {}", k, v)).collect::<Vec<_>>().join(", "))?;
    }
    Ok(())
}

fn rel_path(base: &Path, path: impl AsRef<Path>) -> String {
    pathdiff::diff_paths(path.as_ref(), base).unwrap().to_string_lossy().into_owned()
}

struct TomlStr<S>(S);

impl<S> fmt::Display for TomlStr<S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('\"')?;
        for ch in self.0.to_string().chars() {
            match ch {
                '\u{8}' => f.write_str("\\b")?,
                '\u{9}' => f.write_str("\\t")?,
                '\u{a}' => f.write_str("\\n")?,
                '\u{c}' => f.write_str("\\f")?,
                '\u{d}' => f.write_str("\\r")?,
                '\u{22}' => f.write_str("\\\"")?,
                '\u{5c}' => f.write_str("\\\\")?,
                c if c < '\u{1f}' => write!(f, "\\u{:04X}", ch as u32)?,
                ch => f.write_char(ch)?,
            }
        }
        f.write_char('\"')
    }
}

struct TomlFlatArray<'a, S>(&'a [S]);

impl<'a, S> fmt::Display for TomlFlatArray<'a, S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('[')?;
        for (i, s) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{}", TomlStr(s))?;
        }
        f.write_char(']')
    }
}

struct TomlPrettyArray<'a, S>(&'a [S]);

impl<'a, S> fmt::Display for TomlPrettyArray<'a, S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('[')?;
        if self.0.len() > 1 {
            f.write_char('\n')?;
        }
        for s in self.0 {
            if self.0.len() > 1 {
                f.write_str("    ")?;
            }
            write!(f, "{}", TomlStr(s))?;
            if self.0.len() > 1 {
                f.write_str(",\n")?;
            }
        }
        f.write_char(']')
    }
}

struct TomlVersion<'a>(&'a VersionReq);

impl<'a> fmt::Display for TomlVersion<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = self.0.to_string();
        write!(f, "{}", TomlStr(s.trim_start_matches("^")))
    }
}
