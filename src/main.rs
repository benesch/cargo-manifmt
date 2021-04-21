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

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::error::Error;
use std::fmt::{self, Write as FmtWrite};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use cargo::core::compiler::CrateType;
use cargo::core::dependency::DepKind;
use cargo::core::manifest::TargetKind;
use cargo::core::{Dependency, GitReference, Manifest, Target, Workspace};
use cargo::util::config::Config;
use cargo::util::important_paths;
use cargo::util::interning::InternedString;
use regex_macro::regex;
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
        let extra = parse_manifest(package.manifest_path())?;
        let mut out: Vec<u8> = vec![];
        render_manifest(&mut out, package.root(), manifest, &extra)?;
        fs::write(package.manifest_path(), out)?;
    }
    Ok(())
}

fn parse_manifest(path: &Path) -> io::Result<ManifestExtra> {
    let s = fs::read_to_string(path)?;

    let comments = {
        // WARNING: This is *really* hacky, even by cargo-manifmt standards. We
        // should use a proper comment-preserving TOML parser here, when one is
        // ready. See, for example, https://github.com/matklad/tom.
        let mut comments = HashMap::new();
        let mut current_table = String::new();
        let mut current_comment = String::new();
        for line in s.lines() {
            let line = line.trim();
            if line.starts_with("[") && line.ends_with("]") {
                current_table = line[1..line.len() - 1].to_owned();
            } else if line.starts_with("#") {
                current_comment.push_str(line);
                current_comment.push('\n');
            } else {
                let key: String = line
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
                    .collect();
                if !key.is_empty() {
                    comments.insert(
                        format!("{}.{}", current_table, key),
                        current_comment.clone(),
                    );
                }
                current_comment.clear();
            }
        }
        comments
    };

    let toml: toml::Value = toml::from_str(&s)?;
    let package = toml.get("package").unwrap();
    let get_auto_key = |key| package.get(key).and_then(|v| v.as_bool()).unwrap_or(true);
    Ok(ManifestExtra {
        autobenches: get_auto_key("autobenches"),
        autobins: get_auto_key("autobins"),
        autoexamples: get_auto_key("autoexamples"),
        autotests: get_auto_key("autotests"),
        comments,
    })
}

fn render_manifest<W>(
    w: &mut W,
    base: &Path,
    manifest: &Manifest,
    extra: &ManifestExtra,
) -> io::Result<()>
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
        if readme != "README.md" {
            writeln!(w, "readme = {}", TomlStr(readme))?;
        }
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
    if !extra.autobenches {
        writeln!(w, "autobenches = false")?;
    }
    if !extra.autobins {
        writeln!(w, "autobins = false")?;
    }
    if !extra.autoexamples {
        writeln!(w, "autoexamples = false")?;
    }
    if !extra.autotests {
        writeln!(w, "autotests = false")?;
    }

    let mut lib = None;
    let mut bins = vec![];
    let mut examples = vec![];
    let mut tests = vec![];
    let mut benches = vec![];
    let mut custom_build = None;
    for target in manifest.targets() {
        match target.kind() {
            TargetKind::Lib(_) => lib = Some(target),
            TargetKind::Bin => bins.push(target),
            TargetKind::Test => tests.push(target),
            TargetKind::Bench => benches.push(target),
            TargetKind::ExampleLib(_) => examples.push(target),
            TargetKind::ExampleBin => examples.push(target),
            TargetKind::CustomBuild => custom_build = Some(target),
        }
    }

    if let Some(custom_build) = custom_build {
        let path = rel_path(base, custom_build.src_path().path().unwrap());
        if path != "build.rs" {
            writeln!(w, "build = {}", TomlStr(path))?;
        }
    }

    if let Some(toml::Value::Table(metadata)) = manifest.custom_metadata() {
        render_metadata(w, "package.metadata", metadata)?;
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

    let mut deps: BTreeMap<_, Vec<&Dependency>> = BTreeMap::new();
    let mut dev_deps = vec![];
    let mut build_deps = vec![];
    for dep in manifest.dependencies() {
        match dep.kind() {
            DepKind::Normal => {
                deps.entry(dep.platform()).or_default().push(dep);
            }
            DepKind::Development => dev_deps.push(dep),
            DepKind::Build => build_deps.push(dep),
        }
    }

    for (platform, mut deps) in deps {
        if !deps.is_empty() {
            if let Some(platform) = platform {
                writeln!(w, "\n[target.{}.dependencies]", TomlStr(platform))?;
            } else {
                writeln!(w, "\n[dependencies]")?;
            }
            deps.sort_by_key(|dep| dep.name_in_toml());
            for dep in deps {
                render_dependency(w, base, dep, extra)?;
            }
        }
    }

    if !dev_deps.is_empty() {
        writeln!(w, "\n[dev-dependencies]")?;
        dev_deps.sort_by_key(|dep| dep.name_in_toml());
        for dep in dev_deps {
            render_dependency(w, base, dep, extra)?;
        }
    }

    if !build_deps.is_empty() {
        writeln!(w, "\n[build-dependencies]")?;
        build_deps.sort_by_key(|dep| dep.name_in_toml());
        for dep in build_deps {
            render_dependency(w, base, dep, extra)?;
        }
    }

    if !manifest.summary().features().is_empty() {
        writeln!(w, "\n[features]")?;
        for (name, specs) in manifest.summary().features() {
            let value: Vec<_> = specs
                .iter()
                .map(|s| {
                    let s = s.to_string();
                    match s.strip_prefix("dep:") {
                        None => s,
                        Some(s) => s.to_owned(),
                    }
                })
                .collect();
            if let Some(comment) = extra.comments.get(&format!("features.{}", name)) {
                write!(w, "{}", comment)?;
            }
            writeln!(w, "{} = {}", name, TomlFlatArray(&value))?;
        }
    }

    Ok(())
}

fn render_metadata<W>(w: &mut W, key_prefix: &str, metadata: &toml::value::Table) -> io::Result<()>
where
    W: io::Write,
{
    let mut non_table_buf = Vec::new();
    let mut table_buf = Vec::new();

    for (key, value) in metadata {
        match value {
            toml::Value::Table(table) => {
                let new_prefix = format!("{}.{}", key_prefix, key);
                render_metadata(&mut table_buf, &new_prefix, table)?;
            }
            toml::Value::Array(array) => {
                let mut s = format!("{} = {}", key, TomlFlatArray(array));
                if s.len() > 100 {
                    s = format!("{} = {}", key, TomlPrettyArray(array));
                }
                writeln!(non_table_buf, "{}", s)?;
            }
            _ => writeln!(non_table_buf, "{} = {}", key, value)?,
        }
    }

    if !non_table_buf.is_empty() {
        writeln!(w, "\n[{}]", key_prefix)?;
        w.write(&non_table_buf)?;
    }

    w.write(&table_buf)?;
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
        TargetKind::Bin => {
            path == "src/main.rs"
                || path == format!("src/bin/{}/main.rs", target.name())
                || path == format!("src/bin/{}.rs", target.name())
        }
        TargetKind::Test => {
            path == format!("tests/{}/main.rs", target.name())
                || path == format!("tests/{}.rs", target.name())
        }
        TargetKind::Bench => {
            path == format!("benches/{}/main.rs", target.name())
                || path == format!("benches/{}.rs", target.name())
        }
        TargetKind::ExampleLib(_) | TargetKind::ExampleBin => {
            path == format!("examples/{}/main.rs", target.name())
                || path == format!("examples/{}.rs", target.name())
        }
        _ => false,
    };
    if let TargetKind::Lib(crate_types) = target.kind() {
        for crate_type in crate_types {
            match crate_type {
                CrateType::ProcMacro => writeln!(buf, "proc-macro = true")?,
                _ => (),
            }
        }
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
        writeln!(
            w,
            "\n[{}]",
            match target.kind() {
                TargetKind::Lib(_) => "lib",
                TargetKind::Bin => "[bin]",
                TargetKind::Test => "[test]",
                TargetKind::Bench => "[bench]",
                TargetKind::ExampleLib(_) | TargetKind::ExampleBin => "[example]",
                TargetKind::CustomBuild => unreachable!(),
            }
        )?;
        if !(target.is_lib() && target.name() == package_name) {
            writeln!(w, "name = {}", TomlStr(target.name()))?;
        }
        w.write(&buf)?;
    }
    Ok(())
}

fn render_dependency<W>(
    w: &mut W,
    base: &Path,
    dep: &Dependency,
    extra: &ManifestExtra,
) -> io::Result<()>
where
    W: io::Write,
{
    let toml_key = match dep.platform() {
        None => format!("dependencies.{}", dep.name_in_toml()),
        Some(platform) => format!(
            "target.{}.dependencies.{}",
            TomlStr(platform),
            dep.name_in_toml()
        ),
    };
    if let Some(comment) = extra.comments.get(&toml_key) {
        write!(w, "{}", comment)?;
    }
    write!(w, "{} = ", dep.name_in_toml())?;
    let mut meta: Vec<(&'static str, Box<dyn fmt::Display>)> = vec![];
    if dep.package_name() != dep.name_in_toml() {
        meta.push(("package", Box::new(TomlStr(dep.package_name()))));
    }
    let source_id = dep.source_id();
    if source_id.is_path() {
        let url = source_id.url();
        meta.push(("path", Box::new(TomlStr(rel_path(base, url.path())))));
    } else if let Some(git_ref) = source_id.git_reference() {
        meta.push(("git", Box::new(TomlStr(source_id.url().clone()))));
        match git_ref {
            GitReference::Tag(tag) => meta.push(("tag", Box::new(TomlStr(tag)))),
            GitReference::Branch(branch) if branch != "master" => {
                meta.push(("branch", Box::new(TomlStr(branch))))
            }
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
        meta.push(("optional", Box::new("true")));
    }
    if meta.is_empty() {
        write!(w, "{}\n", TomlVersion(dep.version_req()))?;
    } else {
        if dep.version_req().to_string() != "*" {
            meta.insert(0, ("version", Box::new(TomlVersion(dep.version_req()))));
        }
        write!(
            w,
            "{{ {} }}\n",
            meta.iter()
                .map(|(k, v)| format!("{} = {}", k, v))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    Ok(())
}

fn rel_path(base: &Path, path: impl AsRef<Path>) -> String {
    pathdiff::diff_paths(path.as_ref(), base)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

struct ManifestExtra {
    autobenches: bool,
    autobins: bool,
    autoexamples: bool,
    autotests: bool,
    comments: HashMap<String, String>,
}

struct TomlStr<S>(S);

impl<S> fmt::Display for TomlStr<S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.to_string().fmt_toml(f)
    }
}

struct TomlFlatArray<'a, S>(&'a [S]);

impl<'a, S> fmt::Display for TomlFlatArray<'a, S>
where
    S: TomlDisplay,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('[')?;
        for (i, s) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            s.fmt_toml(f)?;
        }
        f.write_char(']')
    }
}

struct TomlPrettyArray<'a, S>(&'a [S]);

impl<'a, S> fmt::Display for TomlPrettyArray<'a, S>
where
    S: TomlDisplay,
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
            s.fmt_toml(f)?;
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
        let version_regex =
            regex!(r#"\^(?P<major>[0-9]+)(\.(?P<minor>[0-9]+)(\.(?P<patch>[0-9]+))?)?"#);
        if let Some(caps) = version_regex.captures(&s) {
            write!(
                f,
                "\"{}.{}.{}\"",
                caps.name("major").map_or("0", |m| m.as_str()),
                caps.name("minor").map_or("0", |m| m.as_str()),
                caps.name("patch").map_or("0", |m| m.as_str())
            )
        } else {
            write!(f, "{}", TomlStr(s))
        }
    }
}

trait TomlDisplay {
    fn fmt_toml(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

impl TomlDisplay for toml::Value {
    fn fmt_toml(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl TomlDisplay for &str {
    fn fmt_toml(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.contains('"') && !self.contains('\'') {
            f.write_char('\'')?;
            f.write_str(self)?;
            return f.write_char('\'');
        }

        f.write_char('\"')?;
        for ch in self.chars() {
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

impl TomlDisplay for String {
    fn fmt_toml(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt_toml(f)
    }
}

impl TomlDisplay for InternedString {
    fn fmt_toml(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt_toml(f)
    }
}
