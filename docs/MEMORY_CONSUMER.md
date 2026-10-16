# External memory reference consumer

`examples/memory_consumer.py` is a Python 3.11+ **stdlib developer example**, not a
shipped app component, ML sidecar, MCP replacement, or cloud integration. It reads
only the operator-selected API origin and half-open UTC window. It never requests
capture, marks, inference, report generation, scheduling, delivery, or images.

## Verification status

The local SQLite/UTC tests and real refused-TCP error-path test can run now.
**Successful API consumption is not yet verified:** the frame/coverage routes and
real isolated Rust fixture runner must be supplied by the API owner. The seven
`RealApiFixtureTests` are an executable integration harness, but are explicitly
skipped without that runner. Skips are not P1 acceptance. This example targets the
actual `FrameEvidence`, `FramePage`, and `CaptureCoverage` definitions in
`crates/traits/src/history.rs` and the existing `FrameDetail` in
`crates/traits/src/ipc.rs`; it does not implement replacement HTTP responses.

## Choose a bounded read

Create a private directory for the state DB. On POSIX, use restrictive directory
permissions and `umask 077` before creating it; on Windows, choose a directory
whose ACL permits only the intended user. Set `SCREENSEARCH_API_TOKEN` through your
secret manager or shell environment. Prefer the independent read-only credential.
**Never put a bearer token on the command line, in the URL, in documentation, or
in a committed `.env` file.** The example has no token argument.

```sh
# SCREENSEARCH_API_TOKEN is already supplied securely in the environment.
python3 examples/memory_consumer.py \
  --api-url http://127.0.0.1:43210 \
  --state /chosen/private-directory/day.sqlite3 \
  --day 2026-10-25 --timezone Europe/Paris \
  --limit 137 --max-pages 1

# Same source/window/state: resume from the last committed page.
python3 examples/memory_consumer.py \
  --api-url http://127.0.0.1:43210 \
  --state /chosen/private-directory/day.sqlite3 \
  --day 2026-10-25 --timezone Europe/Paris --limit 137
```

Alternatively, supply both `--from` and exclusive `--to`. They accept signed
integral UTC milliseconds or explicit UTC ISO timestamps with at most millisecond
precision. Naive timestamps, non-UTC offsets, floating-point timestamps, excessive
precision, missing bounds and empty/reversed windows are rejected.

```sh
python3 examples/memory_consumer.py \
  --state /chosen/private-directory/explicit-window.sqlite3 \
  --from 2026-10-24T22:00:00.000Z --to 2026-10-25T23:00:00.000Z
```

`--day` requires an explicit IANA `--timezone`; the machine's local zone is never
assumed. Paris spring/autumn transitions produce 23/25-hour windows. Each date
boundary is converted independently, not by adding 24 hours to UTC. Historical
nonexistent local midnights/dates are rejected with instructions to use explicit
bounds. Python needs installed timezone data for named zones; the example does
not download it. `UTC` is also accepted explicitly.

`--api-url` overrides `SCREENSEARCH_API_URL`, otherwise the loopback default above
is used. Only a concrete HTTP(S) origin is allowed: no credentials, non-root path,
query, fragment or wildcard destination. Environment proxies are disabled;
redirects are refused without visiting the redirect destination or forwarding the
token; HTTPS retains normal certificate verification. Plain HTTP exposes the
bearer/history to the chosen network: select the destination deliberately.

## Durability and late enrichment

- One state DB is bound to one exact API origin and selected window. A different
  origin/window needs a different file. No next-day or unbounded-history drain.
- Each page is processed as one SQLite transaction with WAL and
  `synchronous=FULL`. All fact/version writes occur **before the cursor update in
  that same transaction**. Failure rolls back both. A process crash can replay a
  page; retries upsert, rather than append a duplicate fact.
- Facts are keyed by `(history_id, frame_id)`. `original_json` is immutable;
  `current_json` may update only when `captured_at` agrees. Distinct observations
  remain in `versions`; nothing is silently shortened to a search snippet.
- `not_available` means no stored text, not empty OCR or permanent OCR failure.
  Available empty strings are retained as available. Every invocation re-reads
  detail for currently textless IDs, even after its listing checkpoint is saved
  or exhausted. A detail 404 is counted as missing, not converted into empty text.
  A detail read whose identity conflicts with the preserved fact is recorded as a
  conflict, not counted as a successful re-read.
- Detail is the **existing** `/v1/frames/{id}` wire shape, not `FrameEvidence`:
  availability is derived from nullable `raw_text`, vision description is read
  from `vision.description`, and the detail's default `text_source` is ignored
  when text is absent. File paths, image bytes and incidental legacy fields are
  not copied into the consumer facts. Detail does not supply filter version or
  session identity; those remain the most recently observed page values until a
  full re-list refreshes them.
- Exhaustion records the completed traversal time. The next invocation re-lists
  the **same selected window** once `--rescan-seconds` has elapsed (default 3600).
  `--rescan-seconds 0` requests a new traversal immediately when exhausted.
  Already-progressing traversals resume first. There is no automatic timer in a
  one-shot invocation. Use `--watch-seconds 60` to repeat explicitly within the
  same window; Ctrl-C exits safely. A scheduled repeated invocation also works.
- Re-listing also refreshes already-available text, filtering, vision and session
  references. One traversal cannot promise eventual completeness. Facts missing
  from a later traversal are retained as prior observations: this is not a
  tombstone/deletion replication feed or a statement of current availability.

`--max-pages` limits fetched pages per invocation, including a page discarded for
identity reconciliation. A normal drain remains bounded by the chosen time
window and the API's frozen maximum membership. A summary reports fetched versus
successfully committed pages separately. Textless detail re-reads concern only
facts already observed in that selected window.

## Identity conflicts, restores and reconciliation

Coverage's observed `frame_id_high_water` is persisted before and after a drain,
alongside the checkpoint. `highest_high_water` additionally retains the maximum
ever observed locally. A decrease invalidates the cursor, creates an
`allocator_decreased` reconciliation record, and begins a fresh bounded scan.
A 409 (changed history identity) also invalidates the traversal; separate history
namespaces and all their earlier facts are preserved. A `/v1/frames` **400** whose
cursor snapshot maximum exceeds the current allocator is treated the same way
**only after** coverage re-confirms a decrease: the retained cursor is discarded
and a bounded rescan begins. An unexplained 400 is propagated as an explicit HTTP
error, never as empty data and never as an asserted rewind.

If a previously recorded `(history_id, frame_id)` arrives with a different
`captured_at`, the old original **and current** fact stay intact. The competing
observation goes to `versions` and `conflicts`, linked to `reconciliations`.
The old cursor/snapshot are cleared, and the reader restarts the selected window.
During that reconciliation scan, conflicting observations are recorded separately
while pagination progresses. At exhaustion, the record is `scanned_with_conflicts`
(not silently “resolved”). A conflict from a detail read, an end-of-drain rewind,
or a finite `--max-pages` budget can leave reconciliation pending for the next
invocation. A 409 and the changed-history page it triggers are the **same** logical
change and count as one bounded restart, so a single genuine history change
recovers automatically instead of failing on a double count. Repeated distinct
mid-drain identity changes are bounded to one automatic restart per invocation; an
unstable source fails explicitly rather than looping.

**Restore operator action is mandatory even when high-water has caught up:**

```sh
python3 examples/memory_consumer.py \
  --state /chosen/private-directory/day.sqlite3 \
  --day 2026-10-25 --timezone Europe/Paris --reconcile
```

Use the original `--api-url` too if it was nondefault. This records
`operator_restore` and discards only traversal progress, never the old facts.
Use it after backup restoration, old/direct writers, known unsupported imports,
or independently active clone divergence. Inspect the local `reconciliations`,
`conflicts`, `versions`, and immutable `facts.original_json` before making an
operator decision about downstream memory. The example deliberately has no
“accept new identity” or automatic conflict-erasure control. Use a separately
chosen state file if you need a clean baseline while retaining the old file for
audit. Historical conflicts keep the one-shot exit status nonzero.

These are detectable-signal safeguards, **not exactly-once delivery**. Same-ID,
same-timestamp reuse, invisible restores that catch up before observation,
independent clones and unsupported writers may escape detection. API page
membership is bounded, not a frozen content snapshot; retention can remove rows.
There is no cross-restore global uniqueness or eventual completeness guarantee.

## Errors and private state

Stdout contains only a count/status JSON summary, never token, OCR, title, URL,
cursor or server diagnostics. The state DB, WAL/SHM files, versions, conflicts,
coverage JSON and backups can contain **full private evidence** and need the same
protection as the source. The separate `.lock` SQLite file serializes simultaneous
CLI invocations for that state path. Do not commit any generated state or tokens.
No logs/upload/telemetry or model downloads are introduced.

Exit codes: `0` successful bounded work (possibly intentionally paused at the
page budget); `2` invalid input/protocol, retained identity conflicts, or pending
reconciliation; `3` explicit HTTP/connection/read error; `4` local state/lock
failure; `130` interruption. 401/403/503 have credential/readiness guidance. An
HTTP/storage/JSON failure is **never** an empty result or `NO_DATA`. An empty
successful traversal means no recorded frames in the selected query, not that
nothing happened. Capture-enabled intent and frame gaps are not activity proofs.

## Tests now

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s examples -p test_memory_consumer.py -v
```

Local tests use actual temporary SQLite files and transactions. They exercise
reopen/resume, transactional failure rollback, immutable conflict facts, bounded
reconciliation records, explicit restore, decreased allocator, late full text,
available empty versus unavailable text, periodic re-list updates, UTC/DST and
URL validation. A CLI test makes a real refused TCP connection with no HTTP
responder and verifies a non-data error plus unchanged checkpoint. These tests
**do not** claim successful API/router/store integration.

## Real API fixture continuation seam (not implemented here)

The existing `crates/api/tests/http_api.rs` demonstrates a real `TestHost` backed
by `SqliteStore`, real `ApiServer::start`, an ephemeral TCP listener, production
`Store::insert_frame` / `insert_ocr`, and real HTTP requests. Its
`live_server_for_curl` is a fixed-port, time-limited old fixture without the new
routes or required state controls; it is **not** sufficient for this acceptance.
No Python `http.server`, mock transport, response cassette, Rust replacement
router or hardcoded JSON adapter is supplied.

After frame/coverage routes and Store readiness are implemented, the API owner
can supply a dedicated native Rust fixture runner (for example a new ignored
integration test in `crates/api/tests/reference_consumer.rs`) implementing the
following **test-process-only** control seam. Controls are stdin, never additional
product HTTP routes:

1. The harness launches the configured command with no shell. It gives the
   runner a fresh private directory in `SCREENSEARCH_CONSUMER_FIXTURE_DIR` and a
   random bearer in `SCREENSEARCH_API_TOKEN`. The runner must use only an isolated
   real `SqliteStore`, initialize history through the actual production setup,
   bind the actual API/router to an ephemeral loopback TCP port, leave answer
   providers unavailable and schedules off, and never print the token.
2. Seed more than 1000 nonprivate frames in the selected window, including equal
   and out-of-order capture timestamps, one designated textless frame, one
   available-empty frame, and full raw/content strings. Use production inserts,
   except deliberately labeled unsupported identity-conflict injection. Snapshot
   the isolated DB before adding a tail so a real backup restore can lower its
   allocator without discarding the first committed page.
3. Flush one stdout line beginning `MEMORY_FIXTURE `, followed by JSON with
   `event: "ready"`, `api_url`, integral `from`/`to`, `history_id`, the actual
   `expected_frame_ids` array for that window and `textless_frame_id`. Metadata
   comes from the seeded Store, never substitutes for HTTP responses.
4. Read newline JSON stdin controls, perform the real isolated Store operation,
   then flush `MEMORY_FIXTURE ` JSON with `event: "ack"` and matching `command`:
   - `enrich_textless`, with `frame_id`: store nonprivate full raw/content text
     (raw length >4096) through the production text seam. The Python assertion
     compares the consumer against a fresh real detail HTTP read.
   - `rewind`: restore the isolated earlier SQLite backup, preserve history ID
     and original frame timestamps, and rebind/reopen safely at the same API
     origin. Actual coverage must show decreased allocator high-water.
   - `catch_up_high_water`, with `minimum`: allocate additional real frames
     outside the selected window until the allocator is at least that minimum.
   - `reuse_timestamp`, with `frame_id`: deliberately inject unsupported
     same-history/same-ID timestamp reuse, still within the selected window, on
     an existing in-window frame (the timestamp-conflict test uses a nontextless
     frame; targeting the designated textless frame exercises the detail-conflict
     path). This is an isolated adversarial fixture operation, **not** a new
     production writer or a continuity guarantee.
   - `arm_rewind_before_page_read`, with `count`: run the real backup restore
     immediately before the Nth subsequent `/v1/frames` read, so a cursor frozen
     at the pre-rewind snapshot maximum hits the real mid-drain 400.
   - `rotate_history`: swap the live store to a genuinely new history identity in
     a fresh DB slot (production setup), snapshotting the disposable current DB
     first and seeding the new namespace through the production writers, so a
     retained cursor hits the real 409.
   - `stop`: stop server/close isolated Store and exit promptly.
5. Set `SCREENSEARCH_CONSUMER_FIXTURE_CMD` to a JSON argv array for that actual
   runner; prebuild it on the supported native test host if necessary. A proposed
   command shape, **not a runnable existing target**, is
   `["cargo","test","--locked","-p","api","--test","reference_consumer","--","--ignored","--nocapture","live_reference_consumer_fixture"]`.
   Run the unittest command above with that environment. Each of the seven real
   integration tests launches its own fresh fixture and an actual CLI process.
   They cover >1000-frame resume, late text after checkpoint, restore/decrease
   plus explicit reconciliation, original-fact preservation under reuse, a
   mid-drain rewind that invalidates a stale cursor on a real 400, a genuine new
   history after the first page that recovers in one bounded restart, and a
   detail conflict that is not counted as a successful re-read.

Missing runner configuration is reported as seven **UNVERIFIED skips**. A supplied
but broken/missing runner, timeout, failed command, absent route or failed
assertion is a test failure, not a skip and never synthetic success. Native
Windows app coverage, separate-machine HTTP/MCP acceptance and full P1 acceptance
remain separate gates even after this fixture passes.
