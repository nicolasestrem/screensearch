//! Source files and terminal output for the fixture's projects.
//!
//! File bodies are fixed per `(project, file)` (seed-independent templates filled with the
//! project's names, vocabulary and tickets), so revisiting a file shows the same lines and a
//! viewport naturally recurs across frames. Terminal output is drawn per frame from the
//! seeded stream: test runs, git, ripgrep listings and build logs in the project's toolchain.

use super::prng::SplitMix64;
use super::world::{Lang, Project, Ticket, PEOPLE};

/// `retry_budget` -> `RetryBudget`, `NoteList` -> `NoteList`, `node_groups` -> `NodeGroups`.
pub(crate) fn camel(s: &str) -> String {
    s.split(['_', '-', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The identifier stem of a file name: everything before the first dot.
pub(crate) fn stem_of(file_name: &str) -> &str {
    file_name.split('.').next().unwrap_or(file_name)
}

/// The extension of a file name (after the last dot), lowercase.
fn ext_of(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Replaces every `$[name]` placeholder through `lookup`. Unknown names are left verbatim
/// (the unit test below proves every template resolves completely).
fn subst(template: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find("$[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find(']') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push_str("$[");
                        out.push_str(name);
                        out.push(']');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The C# namespace of a file: its directories after `src`/`tests`, dot-joined.
fn cs_namespace(file: &str) -> String {
    let parts: Vec<&str> = file.split('\\').collect();
    let dirs = &parts[..parts.len().saturating_sub(1)];
    dirs.iter().skip(1).copied().collect::<Vec<_>>().join(".")
}

/// The Go package of a file: `main` for `main.go`, otherwise its directory name.
fn go_package(file: &str) -> String {
    let parts: Vec<&str> = file.split('\\').collect();
    let name = parts.last().copied().unwrap_or(file);
    if name == "main.go" || parts.len() < 2 {
        "main".to_string()
    } else {
        parts[parts.len() - 2].to_string()
    }
}

/// Every placeholder value for one project file.
fn vars(p: &Project, file_index: usize) -> impl Fn(&str) -> Option<String> + '_ {
    move |name: &str| {
        let file = p.files[file_index];
        let file_name = p.file_name(file_index);
        let stem = stem_of(file_name);
        let value = match name {
            "stem" => stem.to_string(),
            "Stem" => camel(stem),
            "a" => p.vocab[0].to_string(),
            "b" => p.vocab[1].to_string(),
            "c" => p.vocab[2].to_string(),
            "d" => p.vocab[3].to_string(),
            "A" => camel(p.vocab[0]),
            "B" => camel(p.vocab[1]),
            "C" => camel(p.vocab[2]),
            "D" => camel(p.vocab[3]),
            "D_UP" => p.vocab[3].to_ascii_uppercase(),
            "t1" => p.tickets[0].key.to_string(),
            "t2" => p.tickets[1].key.to_string(),
            "h" => p.tickets[2].key.to_string(),
            "slug" => p.slug.to_string(),
            "ident" => p.ident(),
            "service" => p.service.to_string(),
            "doc" => p.docs[0].to_string(),
            "site" => p.site.to_string(),
            "sp0" => p.site_paths[0].to_string(),
            "tracker" => p.tracker_domain().to_string(),
            "ns" => cs_namespace(file),
            "pkg" => go_package(file),
            _ => return None,
        };
        Some(value)
    }
}

const RUST_TEMPLATE: &[&str] = &[
    "use std::time::{Duration, Instant};",
    "",
    "use crate::$[b]::$[B]Policy;",
    "",
    "/// $[Stem] handling for $[service] ($[t1]).",
    "#[derive(Debug, Clone)]",
    "pub struct $[Stem]$[A] {",
    "    pub max_$[a]: u32,",
    "    pub $[c]_base: Duration,",
    "    pub $[c]_cap: Duration,",
    "    window_started: Instant,",
    "}",
    "",
    "impl $[Stem]$[A] {",
    "    pub fn new(max_$[a]: u32, policy: &$[B]Policy) -> Self {",
    "        let $[c]_cap = policy.cap().min(Duration::from_secs(30));",
    "        Self { max_$[a], $[c]_base: Duration::from_millis(200), $[c]_cap, window_started: Instant::now() }",
    "    }",
    "",
    "    /// See $[doc] for the policy; $[h] caps the delay.",
    "    pub fn next_$[c](&self, attempt: u32, $[d]: f64) -> Duration {",
    "        let exp = self.$[c]_base.mul_f64(2f64.powi(attempt as i32));",
    "        exp.min(self.$[c]_cap).mul_f64(1.0 + $[d])",
    "    }",
    "",
    "    pub fn record_429(&mut self, now: Instant) -> bool {",
    "        if now.duration_since(self.window_started) > Duration::from_secs(60) {",
    "            self.window_started = now;",
    "        }",
    "        self.max_$[a] > 0",
    "    }",
    "}",
    "",
    "#[cfg(test)]",
    "mod tests {",
    "    #[test]",
    "    fn $[c]_caps_at_max_for_$[stem]() {",
    "        let policy = super::$[B]Policy::default();",
    "        let p = super::$[Stem]$[A]::new(3, &policy);",
    "        assert_eq!(p.next_$[c](10, 0.0), std::time::Duration::from_secs(30));",
    "    }",
    "}",
];

const PYTHON_TEMPLATE: &[&str] = &[
    "from __future__ import annotations",
    "",
    "import csv",
    "import logging",
    "from dataclasses import dataclass",
    "",
    "log = logging.getLogger('$[ident].$[stem]')",
    "",
    "",
    "@dataclass",
    "class $[Stem]$[A]:",
    "    $[a]_id: str",
    "    $[b]_count: int",
    "    $[c]: str | None = None",
    "",
    "",
    "def parse_$[stem]_$[a]s(path: str) -> list[$[Stem]$[A]]:",
    "    # $[t1]: strip the UTF-8 BOM before reading the header row",
    "    with open(path, encoding='utf-8-sig', newline='') as handle:",
    "        rows = list(csv.DictReader(handle))",
    "    log.info('parsed %d $[a] rows from %s', len(rows), path)",
    "    return [$[Stem]$[A]($[a]_id=r['id'], $[b]_count=int(r['count'])) for r in rows]",
    "",
    "",
    "def validate_$[b]_$[stem](items: list[$[Stem]$[A]]) -> None:",
    "    seen: set[str] = set()",
    "    for item in items:",
    "        if item.$[a]_id in seen:",
    "            raise ValueError(f'duplicate $[a] {item.$[a]_id} ($[t2])')",
    "        seen.add(item.$[a]_id)",
    "",
    "",
    "def $[d]_window_for_$[stem](days: int = 7) -> tuple[str, str]:",
    "    # $[h]: make the $[d] window a parameter instead of a constant",
    "    return ('now() - interval %d day' % days, 'now()')",
];

const TS_TEMPLATE: &[&str] = &[
    "import { $[B]Store } from './$[b]';",
    "import type { $[A]Event } from '../types';",
    "",
    "// $[t1]: replay must stay idempotent across reconnects",
    "export interface $[Stem]Options {",
    "  max$[A]Retries: number;",
    "  $[c]TimeoutMs: number;",
    "}",
    "",
    "export class $[Stem]$[A] {",
    "  private pending: $[A]Event[] = [];",
    "",
    "  constructor(private readonly store: $[B]Store, private readonly opts: $[Stem]Options) {}",
    "",
    "  enqueue(event: $[A]Event): void {",
    "    if (this.pending.some((e) => e.id === event.id)) return;",
    "    this.pending.push(event);",
    "  }",
    "",
    "  async flush$[C](): Promise<number> {",
    "    const batch = this.pending.splice(0, this.pending.length);",
    "    for (const event of batch) {",
    "      await this.store.apply$[D](event, { timeoutMs: this.opts.$[c]TimeoutMs });",
    "    }",
    "    return batch.length;",
    "  }",
    "}",
    "",
    "// $[h]: expose this through the public $[slug] entry point",
    "export function create$[Stem](store: $[B]Store): $[Stem]$[A] {",
    "  return new $[Stem]$[A](store, { max$[A]Retries: 3, $[c]TimeoutMs: 5000 });",
    "}",
];

const GO_TEMPLATE: &[&str] = &[
    "package $[pkg]",
    "",
    "import (",
    "\t\"context\"",
    "\t\"fmt\"",
    "\t\"time\"",
    ")",
    "",
    "// $[Stem]$[A] tracks $[a] state for $[service] ($[t1]).",
    "type $[Stem]$[A] struct {",
    "\t$[b]Timeout time.Duration",
    "\tmax$[C] int",
    "}",
    "",
    "// New$[Stem]$[A] returns the defaults agreed in $[doc].",
    "func New$[Stem]$[A]() *$[Stem]$[A] {",
    "\treturn &$[Stem]$[A]{$[b]Timeout: 5 * time.Second, max$[C]: 3}",
    "}",
    "",
    "// Run$[A] blocks until the $[a] reaches a terminal phase or ctx ends.",
    "func (s *$[Stem]$[A]) Run$[A](ctx context.Context, name string) error {",
    "\tfor attempt := 0; attempt < s.max$[C]; attempt++ {",
    "\t\tif err := s.check$[B](ctx, name); err == nil {",
    "\t\t\treturn nil",
    "\t\t}",
    "\t\ttime.Sleep(time.Duration(attempt+1) * s.$[b]Timeout)",
    "\t}",
    "\treturn fmt.Errorf(\"$[a] %s: $[b] still failing after %d attempts ($[h])\", name, s.max$[C])",
    "}",
    "",
    "func (s *$[Stem]$[A]) check$[B](ctx context.Context, name string) error {",
    "\tctx, cancel := context.WithTimeout(ctx, s.$[b]Timeout)",
    "\tdefer cancel()",
    "\treturn probe$[D](ctx, name)",
    "}",
];

const CSHARP_TEMPLATE: &[&str] = &[
    "using System;",
    "using System.Collections.Generic;",
    "",
    "namespace $[ns];",
    "",
    "/// <summary>$[Stem] rules for $[service] ($[t1]).</summary>",
    "public sealed class $[Stem]$[A]",
    "{",
    "    private readonly List<decimal> _$[b]s = new();",
    "",
    "    public void Add$[B](decimal value) => _$[b]s.Add(value);",
    "",
    "    public decimal Round$[D](decimal $[d]) =>",
    "        Math.Round($[d], 2, MidpointRounding.ToEven);",
    "",
    "    public decimal Total$[B]For$[Stem]()",
    "    {",
    "        decimal total = 0m;",
    "        foreach (var value in _$[b]s)",
    "        {",
    "            total += Round$[D](value);",
    "        }",
    "        return total; // $[h]: banker's rounding, see $[doc]",
    "    }",
    "",
    "    public bool Matches$[C](decimal ledgerTotal) =>",
    "        Math.Abs(Total$[B]For$[Stem]() - ledgerTotal) < 0.005m; // $[t2]",
    "}",
];

const KOTLIN_TEMPLATE: &[&str] = &[
    "package com.contoso.pocketbank",
    "",
    "import kotlinx.coroutines.flow.MutableStateFlow",
    "import kotlinx.coroutines.flow.StateFlow",
    "",
    "// $[t1]: never re-prompt after an explicit cancel",
    "class $[Stem]$[A](private val api: $[B]Api) {",
    "    private val _state = MutableStateFlow<$[A]State>($[A]State.Idle)",
    "    val state: StateFlow<$[A]State> = _state",
    "",
    "    suspend fun submit$[A](amountCents: Long): Result<Unit> {",
    "        if (amountCents > MAX_$[D_UP]_CENTS) return Result.failure(IllegalArgumentException(\"over the $[c] cap\"))",
    "        _state.value = $[A]State.Loading",
    "        return api.post$[A](amountCents).also { _state.value = $[A]State.Done }",
    "    }",
    "",
    "    fun on$[D]Cancelled() {",
    "        _state.value = $[A]State.Cancelled // $[t1]",
    "    }",
    "",
    "    companion object {",
    "        const val MAX_$[D_UP]_CENTS = 500_000L // see $[doc]",
    "    }",
    "}",
];

const GRADLE_TEMPLATE: &[&str] = &[
    "plugins {",
    "    id(\"com.android.application\")",
    "    id(\"org.jetbrains.kotlin.android\")",
    "}",
    "",
    "android {",
    "    namespace = \"com.contoso.pocketbank\"",
    "    compileSdk = 34 // $[h] bumps this to 35",
    "    defaultConfig {",
    "        applicationId = \"com.contoso.pocketbank\"",
    "        minSdk = 26",
    "        targetSdk = 34",
    "        versionCode = 512",
    "        versionName = \"5.12.0\"",
    "    }",
    "}",
    "",
    "dependencies {",
    "    implementation(\"androidx.biometric:biometric:1.2.0-alpha05\") // $[t1]",
    "    implementation(\"org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1\")",
    "    testImplementation(\"junit:junit:4.13.2\")",
    "}",
];

const TERRAFORM_TEMPLATE: &[&str] = &[
    "terraform {",
    "  required_providers {",
    "    aws = {",
    "      source  = \"hashicorp/aws\"",
    "      version = \"~> 5.0\" # $[h]",
    "    }",
    "  }",
    "}",
    "",
    "module \"$[stem]\" {",
    "  source = \"../../modules/vpc\"",
    "",
    "  name            = \"contoso-$[stem]\"",
    "  cidr            = \"10.40.0.0/16\"",
    "  azs             = [\"eu-west-1a\", \"eu-west-1b\", \"eu-west-1c\"]",
    "  private_subnets = [\"10.40.1.0/24\", \"10.40.2.0/24\", \"10.40.3.0/24\"]",
    "}",
    "",
    "resource \"aws_eip\" \"nat_$[stem]\" {",
    "  count  = 2",
    "  domain = \"vpc\"",
    "  tags = {",
    "    Name   = \"contoso-$[stem]-nat-${count.index}\"",
    "    Ticket = \"$[t1]\"",
    "  }",
    "}",
];

const TFVARS_TEMPLATE: &[&str] = &[
    "environment         = \"staging\"",
    "cluster_name        = \"contoso-staging\"",
    "node_instance_types = [\"m6i.large\"]",
    "node_min_size       = 2",
    "node_max_size       = 4 # $[t2]: raise for the load test",
    "node_desired_size   = 2",
    "enable_nat_gateway  = true",
    "single_nat_gateway  = true",
    "",
    "tags = {",
    "  Owner  = \"platform\"",
    "  Ticket = \"$[t2]\"",
    "}",
];

const MARKDOWN_TEMPLATE: &[&str] = &[
    "# $[Stem]",
    "",
    "> Owner: Engineering Handbook team. Tracked in $[t1].",
    "",
    "## When this applies",
    "",
    "- You are the primary on-call engineer for $[service].",
    "- An alert pages you outside business hours.",
    "",
    "## Escalation for $[stem]",
    "",
    "1. Acknowledge the page within 5 minutes.",
    "2. Post in the incidents channel with the alert link.",
    "3. Escalate to the secondary after 15 minutes without progress.",
    "",
    "## See also",
    "",
    "- $[doc]",
    "- https://$[site]$[sp0]",
];

const TOML_TEMPLATE: &[&str] = &[
    "[site]",
    "title = 'Northwind Engineering Handbook'",
    "base_url = 'https://$[site]'",
    "default_language = 'en'",
    "",
    "[search]",
    "index = 'public/search-index.json'",
    "exclude = ['drafts/**', 'archive/**']",
    "",
    "[links]",
    "check_external = true",
    "ignore = ['https://$[tracker]/*']  # $[t2]",
];

fn template_for(file_name: &str) -> &'static [&'static str] {
    match ext_of(file_name).as_str() {
        "rs" => RUST_TEMPLATE,
        "py" => PYTHON_TEMPLATE,
        "ts" | "tsx" => TS_TEMPLATE,
        "go" => GO_TEMPLATE,
        "cs" => CSHARP_TEMPLATE,
        "kt" => KOTLIN_TEMPLATE,
        "kts" => GRADLE_TEMPLATE,
        "tf" => TERRAFORM_TEMPLATE,
        "tfvars" => TFVARS_TEMPLATE,
        "md" => MARKDOWN_TEMPLATE,
        _ => TOML_TEMPLATE,
    }
}

/// The full body of a project file (fixed per file).
pub(crate) fn file_body(p: &Project, file_index: usize) -> Vec<String> {
    let lookup = vars(p, file_index);
    template_for(p.file_name(file_index))
        .iter()
        .map(|line| subst(line, &lookup))
        .collect()
}

/// One editor viewport: up to `rows` lines starting at one of three fixed scroll anchors,
/// each prefixed with its line number the way the gutter reads under OCR. Returns the
/// viewport and the 1-based line number of its first row.
pub(crate) fn viewport(
    p: &Project,
    file_index: usize,
    anchor: usize,
    rows: usize,
) -> (Vec<String>, usize) {
    let body = file_body(p, file_index);
    let anchors = [0usize, 9, 18];
    let start = anchors[anchor % anchors.len()].min(body.len().saturating_sub(1));
    let lines = body
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .map(|(i, line)| {
            let text = line.trim();
            if text.is_empty() {
                format!("{}", i + 1)
            } else {
                format!("{} {}", i + 1, text)
            }
        })
        .collect();
    (lines, start + 1)
}

/// The prompt PowerShell prints in a project clone.
pub(crate) fn prompt(p: &Project, command: &str) -> String {
    format!("PS {}> {command}", p.dir)
}

/// The two `git remote -v` lines (they carry the repository and its host as a full URL).
pub(crate) fn git_remote(p: &Project) -> Vec<String> {
    let url = p.https_clone_url();
    vec![
        prompt(p, "git remote -v"),
        format!("origin {url} (fetch)"),
        format!("origin {url} (push)"),
    ]
}

/// A ripgrep listing over the clone: every source file as an absolute path.
pub(crate) fn rg_listing(p: &Project) -> Vec<String> {
    let mut out = vec![prompt(p, &format!("rg -l {} {}", p.vocab[0], p.dir))];
    for i in 0..p.files.len() {
        out.push(p.file_path(i));
    }
    out
}

fn hex(rng: &mut SplitMix64, digits: usize) -> String {
    let mut s = format!("{:016x}", rng.next_u64());
    while s.len() < digits {
        s.push_str(&format!("{:016x}", rng.next_u64()));
    }
    s.truncate(digits);
    s
}

/// A short commit hash.
pub(crate) fn short_hash(rng: &mut SplitMix64) -> String {
    hex(rng, 7)
}

/// A request/trace id.
pub(crate) fn request_id(rng: &mut SplitMix64) -> String {
    hex(rng, 8)
}

/// `git log --oneline` with commits that cite the project's tickets.
pub(crate) fn git_log(p: &Project, rng: &mut SplitMix64) -> Vec<String> {
    let mut out = vec![prompt(p, "git log --oneline -4")];
    let subjects = [
        format!(
            "fix({}): {} ({})",
            p.vocab[0],
            p.tickets[0].title.to_lowercase(),
            p.tickets[0].key
        ),
        format!(
            "feat({}): {} ({})",
            p.vocab[1],
            p.tickets[1].title.to_lowercase(),
            p.tickets[1].key
        ),
        format!("{} ({})", p.tickets[2].title, p.tickets[2].key),
        format!("chore: bump {} dependencies", p.slug),
    ];
    for subject in subjects {
        out.push(format!("{} {subject}", short_hash(rng)));
    }
    out
}

/// `git status` on a topic branch named after a ticket.
pub(crate) fn git_status(p: &Project, file_index: usize, rng: &mut SplitMix64) -> Vec<String> {
    let ticket = p.tickets[rng.below(2)].key.to_ascii_lowercase();
    vec![
        prompt(p, "git status -sb"),
        format!(
            "## fix/{ticket}-{}...origin/main [ahead 1]",
            p.vocab[rng.below(4)]
        ),
        format!(" M {}", p.file_url_path(file_index)),
        format!(" M {}", p.file_url_path((file_index + 1) % p.files.len())),
    ]
}

fn test_names(p: &Project, file_index: usize) -> [String; 3] {
    let stem = stem_of(p.file_name(file_index)).to_ascii_lowercase();
    [
        format!("{}_caps_at_max_for_{stem}", p.vocab[2]),
        format!("{}_survives_reconnect", p.vocab[0]),
        format!("{}_{}_round_trip", p.vocab[1], p.vocab[3]),
    ]
}

/// A test run in the project's toolchain, focused on `file_index`, failing or passing.
/// Every variant prints the absolute path of the file or the clone it ran in.
pub(crate) fn test_run(
    p: &Project,
    file_index: usize,
    failing: bool,
    rng: &mut SplitMix64,
) -> Vec<String> {
    let names = test_names(p, file_index);
    let total = 9 + rng.below(30);
    let failed = usize::from(failing);
    let passed = total - failed;
    let secs = format!("{}.{:02}", rng.below(6), rng.below(100));
    let path = p.file_path(file_index);
    let line = 20 + rng.below(20);
    let stem = stem_of(p.file_name(file_index));
    match p.lang {
        Lang::Rust => {
            let mut out = vec![
                prompt(p, &format!("cargo test {stem}")),
                format!("Compiling {} v0.4.2 ({})", p.slug, p.dir),
                format!("running {total} tests"),
                format!("test tests::{} ... ok", names[1]),
            ];
            if failing {
                out.push(format!("test tests::{} ... FAILED", names[0]));
                out.push(format!(
                    "thread 'tests::{}' panicked at {path}:{line}:9",
                    names[0]
                ));
            }
            out.push(format!(
                "test result: {}. {passed} passed; {failed} failed; 0 ignored; finished in {secs}s",
                if failing { "FAILED" } else { "ok" }
            ));
            out
        }
        Lang::Python => {
            // pytest runs the project's test module; the traceback points into the source.
            let tests = p.file_url_path(p.files.len() - 1);
            let mut out = vec![prompt(p, &format!("python -m pytest {tests} -q"))];
            if failing {
                out.push(format!(
                    "FAILED {tests}::test_{} - AssertionError",
                    names[0]
                ));
                out.push(format!(
                    "File {path}, line {line}, in parse_{stem}_{}s",
                    p.vocab[0]
                ));
                out.push(format!("{failed} failed, {passed} passed in {secs}s"));
            } else {
                out.push(format!("{passed} passed in {secs}s"));
                out.push(format!("rootdir: {}", p.dir));
            }
            out
        }
        Lang::Docs => {
            let mut out = vec![prompt(p, r"python scripts\build_index.py --check-links")];
            if failing {
                out.push(format!(
                    "BROKEN {} -> {} (404)",
                    p.file_url_path(file_index),
                    p.site_url(1)
                ));
                out.push(format!("{} broken links in {path}", 1 + rng.below(3)));
            } else {
                out.push(format!(
                    "Indexed {} pages, 0 broken links",
                    100 + rng.below(200)
                ));
                out.push(format!(r"Wrote {}\public\search-index.json", p.dir));
            }
            out
        }
        Lang::TypeScript => {
            let mut out = vec![prompt(p, &format!("npm test -- {stem}"))];
            if failing {
                out.push(format!("FAIL {}", p.file_url_path(file_index)));
                out.push(format!(
                    "at {}{} ({path}:{line}:13)",
                    camel(stem),
                    camel(p.vocab[0])
                ));
            } else {
                out.push(format!("PASS {}", p.file_url_path(file_index)));
            }
            out.push(format!(
                "Tests: {failed} failed, {passed} passed, {total} total"
            ));
            out
        }
        Lang::Go => {
            let pkg = p.files[file_index]
                .rsplit_once('\\')
                .map_or("", |(dir, _)| dir)
                .replace('\\', "/");
            let mut out = vec![prompt(p, &format!("go test ./{pkg}/..."))];
            if failing {
                out.push(format!(
                    "--- FAIL: Test{}{} ({secs}s)",
                    camel(stem),
                    camel(p.vocab[0])
                ));
                out.push(format!(
                    "{path}:{line}: expected phase Promoted, got Waiting"
                ));
                out.push(format!("FAIL {}/{pkg} {secs}s", p.repo));
            } else {
                out.push(format!("ok {}/{pkg} {secs}s", p.repo));
                out.push(format!("PS {}>", p.dir));
            }
            out
        }
        Lang::CSharp => {
            let mut out = vec![prompt(p, "dotnet test --nologo")];
            if failing {
                out.push(format!(
                    "Failed {}Tests.{} [12 ms]",
                    camel(stem),
                    camel(&names[0])
                ));
                out.push(format!(
                    "at {}.{}() in {path}:line {line}",
                    camel(stem),
                    camel(&names[0])
                ));
                out.push(format!(
                    "Failed! - Failed: 1, Passed: {passed}, Skipped: 0, Total: {total}"
                ));
            } else {
                out.push(format!(
                    "Passed! - Failed: 0, Passed: {passed}, Skipped: 0, Total: {total}"
                ));
                out.push(format!("Test run for {}", p.dir));
            }
            out
        }
        Lang::Kotlin => {
            let mut out = vec![prompt(p, ".\\gradlew testDebugUnitTest")];
            if failing {
                out.push(format!("{}Test > {} FAILED", camel(stem), names[0]));
                out.push(format!("e: {path}: ({line}, 17): unresolved reference"));
            } else {
                out.push(format!("BUILD SUCCESSFUL in {secs}s"));
                out.push(format!("Project directory: {}", p.dir));
            }
            out.push(format!("{total} tests completed, {failed} failed"));
            out
        }
        Lang::Terraform => {
            let env_dir = p.files[file_index]
                .rsplit_once('\\')
                .map_or(p.dir.to_string(), |(dir, _)| format!(r"{}\{dir}", p.dir));
            let mut out = vec![
                format!("PS {env_dir}> terraform plan -out tfplan"),
                format!("Reading {path}"),
            ];
            if failing {
                out.push(format!(
                    "Error: Unsupported argument on {} line {line}",
                    p.file_name(file_index)
                ));
            } else {
                out.push(format!(
                    "Plan: {} to add, {} to change, 0 to destroy.",
                    rng.below(4),
                    1 + rng.below(3)
                ));
            }
            out
        }
    }
}

/// A Claude Code transcript excerpt in the terminal: one ticket, worked on in one file.
pub(crate) fn claude_session(
    p: &Project,
    file_index: usize,
    ticket: &Ticket,
    rng: &mut SplitMix64,
) -> Vec<String> {
    let rel = p.files[file_index];
    let additions = 2 + rng.below(9);
    let removals = 1 + rng.below(4);
    let removal_word = if removals == 1 { "removal" } else { "removals" };
    vec![
        format!("> look at {} and fix it in {rel}", ticket.key),
        format!("Read({rel})"),
        format!(
            "The issue is in next_{}: the {} is applied before the cap.",
            p.vocab[2], p.vocab[3]
        ),
        format!("Update({rel})"),
        format!("Updated {rel} with {additions} additions and {removals} {removal_word}"),
        format!("Bash(cd {} and run the tests)", p.dir),
        format!(
            "All {} tests pass. {} is fixed: {}.",
            8 + rng.below(30),
            ticket.key,
            ticket.title.to_lowercase()
        ),
    ]
}

/// A build log line for CI pages, naming the failing step.
pub(crate) fn ci_steps(p: &Project, build: u64) -> Vec<String> {
    vec![
        format!("Build {build} {} main", p.owner_repo()),
        "Checkout Install toolchain Restore cache Build Test Publish".to_string(),
        format!(
            "Step Test failed after 3m 12s on agent win-runner-{}",
            build % 7
        ),
    ]
}

/// A colleague drawn from the project team.
pub(crate) fn teammate(p: &Project, rng: &mut SplitMix64) -> &'static str {
    PEOPLE[p.team[rng.below(p.team.len())]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::world::PROJECTS;

    #[test]
    fn every_template_resolves_every_placeholder() {
        for p in &PROJECTS {
            for i in 0..p.files.len() {
                for line in file_body(p, i) {
                    assert!(!line.contains("$["), "{} {}: {line}", p.slug, p.files[i]);
                }
            }
        }
    }

    #[test]
    fn camel_and_stems_follow_identifier_rules() {
        assert_eq!(camel("retry_budget"), "RetryBudget");
        assert_eq!(camel("NoteList"), "NoteList");
        assert_eq!(stem_of("Button.stories.tsx"), "Button");
        assert_eq!(
            cs_namespace(r"src\Paylane.Api\Controllers\InvoiceController.cs"),
            "Paylane.Api.Controllers"
        );
        assert_eq!(go_package(r"internal\deploy\rollout.go"), "deploy");
        assert_eq!(go_package(r"cmd\orbitctl\main.go"), "main");
    }
}
