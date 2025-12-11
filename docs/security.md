# Security & Privacy

This document outlines the security architecture, privacy controls, and best practices for ScreenSearch.

---

## 🔒 Security Overview

ScreenSearch is designed with **privacy-first** and **local-only** principles:

- ✅ All data stored locally (no cloud uploads)
- ✅ Localhost-only API (127.0.0.1:3131)
- ✅ Application exclusion lists
- ✅ Query input sanitization
- ✅ CORS configuration
- ✅ XSS prevention

---

## 🛡️ Privacy Controls

### 1. Application Exclusion

**Purpose**: Prevent capturing sensitive applications (password managers, banking apps, etc.)

**Configuration** (`config.toml`):
```toml
[privacy]
excluded_apps = ["1Password", "KeePass", "Bitwarden", "Authy"]
```

**How It Works**:
- Screen capture checks active window title before capturing
- Frames from excluded apps are skipped entirely
- No screenshots or OCR data stored for excluded applications

**Implementation**: `screen-capture/src/capture.rs` (lines 85-120)

**Adding Exclusions**:
```rust
// In config.toml
excluded_apps = ["YourApp", "AnotherApp"]
```

### 2. Pause on Lock

**Purpose**: Stop capturing when screen is locked

**Configuration**:
```toml
[privacy]
pause_on_lock = true  # Default: true
```

**Behavior**:
- Automatically pauses capture when Windows locks
- Resumes when user unlocks
- No data captured during locked sessions

### 3. Data Retention

**Purpose**: Automatically delete old data

**Configuration**:
```toml
[database]
retention_days = 30  # Delete data older than 30 days
```

**Cleanup**:
- Runs daily at midnight
- Deletes frames, OCR text, and tags older than retention period
- Configurable via settings API

**API**:
```bash
# Update retention policy
curl -X POST http://localhost:3131/api/settings \
  -H "Content-Type: application/json" \
  -d '{"retention_days": 7}'
```

---

## 🔐 Query Sanitization

### FTS5 Injection Prevention

**Problem**: SQLite FTS5 queries support special operators (`AND`, `OR`, `*`, `"`) that can break or manipulate searches.

**Solution**: All user queries are sanitized before FTS5 MATCH operations.

**Implementation**: `screen-db/src/queries.rs` (lines 450-485)

```rust
pub fn sanitize_fts5_query(query: &str) -> String {
    // Escape FTS5 special characters
    let escaped = query
        .replace("\"", "\"\"")  // Escape quotes
        .replace("*", "")       // Remove wildcards
        .replace("AND", "and")  // Lowercase operators
        .replace("OR", "or")
        .trim()
        .to_string();

    // Wrap in quotes for literal matching
    format!("\"{}\"", escaped)
}
```

**Example**:
```rust
// User input: C++ OR malicious*
// Sanitized: "C++ or malicious"
// Safe FTS5 query
```

**Why This Matters**:
- Prevents query injection attacks
- Handles special characters correctly (e.g., `C++`, `$100`, `@user`)
- Ensures predictable search behavior

### API Input Validation

**All API endpoints validate input**:

1. **String lengths**:
   - `tag_name`: max 200 chars
   - `description`: max 1000 chars
   - `search queries`: max 500 chars

2. **Numeric ranges**:
   - `capture_interval`: >= 1 second
   - `retention_days`: >= 1 day
   - `limit`: 1-1000 results

3. **Format validation**:
   - `hex colors`: `#RRGGBB` or `#RRGGBBAA` regex
   - `timestamps`: ISO 8601 format
   - `tag_ids`: comma-separated integers

**Implementation**: `screen-api/src/handlers/` (validation middleware)

---

## 🏠 Local-Only Architecture

### No Cloud Uploads

**Design Principle**: All data stays on your machine.

- ✅ SQLite database stored locally
- ✅ Screenshots saved to local filesystem
- ✅ No network calls except localhost API
- ✅ No telemetry or analytics

### Localhost-Only API

**Binding**: API server binds to `127.0.0.1:3131` only.

**Configuration** (`config.toml`):
```toml
[api]
host = "127.0.0.1"  # Localhost only, NOT 0.0.0.0
port = 3131
```

**Security Implications**:
- API not accessible from network
- Only local applications can connect
- Firewall rules not needed (localhost bypass)

**Warning**: Changing host to `0.0.0.0` exposes API to network. **Not recommended**.

---

## 🗄️ Data Protection

### Database Encryption

**Current**: SQLite database is **not encrypted** by default.

**Enabling Encryption** (SQLCipher):

1. Install SQLCipher extension
2. Update `screen-db/Cargo.toml`:
   ```toml
   sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "sqlite-cipher"] }
   ```

3. Set encryption key:
   ```rust
   // In database initialization
   sqlx::query("PRAGMA key = 'your-encryption-key'")
       .execute(&pool)
       .await?;
   ```

**Recommendation**: Enable encryption if storing sensitive data.

### Secure Deletion

**Problem**: SQLite VACUUM doesn't securely wipe deleted data.

**Solution** (optional):

1. **Enable secure_delete pragma**:
   ```sql
   PRAGMA secure_delete = ON;
   ```

2. **Use VACUUM after deletes**:
   ```rust
   db.execute("VACUUM").await?;
   ```

**Trade-offs**:
- Secure_delete: ~10% performance overhead
- VACUUM: Temporary disk space usage

**Recommendation**: Enable for high-security scenarios.

### Screenshot Handling

**Current Implementation**:
- Screenshots stored in `captures/` directory
- No encryption by default
- Accessible by local user only

**Hardening Options**:

1. **Windows EFS (Encrypting File System)**:
   - Right-click `captures/` → Properties → Advanced → Encrypt
   - Transparent encryption tied to user account

2. **BitLocker**:
   - Full-disk encryption (if available)
   - Protects all data including captures

3. **VeraCrypt Volume**:
   - Create encrypted container
   - Mount as drive, set as capture directory

---

## 🌐 Web UI Security

### CORS Configuration

**Purpose**: Control which origins can access the API.

**Implementation**: `screen-api/src/routes.rs` (lines 45-70)

```rust
let cors = CorsLayer::new()
    .allow_origin("http://localhost:5173".parse::<HeaderValue>()?)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
    .allow_credentials(true);
```

**Security Rules**:
- ✅ Explicit allow-list (not wildcard `*`)
- ✅ Credentials enabled (cookies, auth headers)
- ✅ Specific HTTP methods only
- ✅ Limited headers

**Production**:
```rust
// For embedded UI (served from same origin)
.allow_origin("http://localhost:3131".parse::<HeaderValue>()?)
```

### XSS Prevention

**Eliminated Vulnerabilities**:

1. **React Safe Rendering**:
   - No `dangerouslySetInnerHTML` usage
   - All text rendered through React elements
   - Automatic HTML escaping

2. **Search Highlighting** (Safe Implementation):
   ```typescript
   // OLD (VULNERABLE):
   <div dangerouslySetInnerHTML={{ __html: highlighted }} />

   // NEW (SAFE):
   {parts.map((part, i) =>
     isMatch(part) ? <mark key={i}>{part}</mark> : part
   )}
   ```

3. **API Response Sanitization**:
   - All user-generated content escaped
   - No raw HTML in responses
   - JSON-only responses (not HTML)

### Content Security Policy

**Recommended CSP Headers** (add to API middleware):

```rust
.layer(SetResponseHeaderLayer::overriding(
    header::CONTENT_SECURITY_POLICY,
    HeaderValue::from_static(
        "default-src 'self'; \
         script-src 'self'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; \
         connect-src 'self';"
    ),
))
```

**Protection**:
- Prevents inline script injection
- Restricts resource loading to same origin
- Blocks unauthorized API calls

---

## 🚀 Deployment Security

### Running as Service

**Avoid running as Administrator/SYSTEM**.

**Recommended**: Run as limited user account.

**Windows Service Setup**:
```powershell
# Create limited service account
net user ScreenMemoryService /add /passwordreq:yes

# Grant minimum permissions
icacls "C:\Program Files\ScreenMemory" /grant ScreenMemoryService:(OI)(CI)RX

# Install service
sc create ScreenMemory binPath= "C:\...\screensearch.exe" obj= .\ScreenMemoryService
```

### Firewall Recommendations

**No firewall rules needed** (localhost-only API).

**If exposing to network** (NOT RECOMMENDED):

```powershell
# Allow inbound on port 3131 (Advanced Firewall)
netsh advfirewall firewall add rule name="ScreenSearch API" ^
    dir=in action=allow protocol=TCP localport=3131
```

**Better**: Use reverse proxy (nginx, caddy) with authentication.

### File Permissions

**Restrict database and capture directories**:

```powershell
# Windows ACLs - Owner-only access
icacls screensearch.db /inheritance:r /grant:r "%USERNAME%":(F)
icacls captures\ /inheritance:r /grant:r "%USERNAME%":(OI)(CI)(F)
```

**Prevents**:
- Other users reading your data
- Unauthorized modifications
- Data exfiltration by malware (running as different user)

### Process Isolation

**Sandboxing Options**:

1. **Windows Sandbox** (Testing):
   - Full VM isolation
   - No persistence after close

2. **App Container** (Advanced):
   ```rust
   // Requires Windows capability integration
   use windows::Security::Authorization::AppCapabilityAccess;
   ```

3. **User Account Control**:
   - Run with standard user (not admin)
   - Prompt for elevation only when needed

---

## 🔍 Security Audit Checklist

### Application Security

- [ ] Excluded apps configured (password managers, banking)
- [ ] Pause on lock enabled
- [ ] Data retention policy set
- [ ] API bound to 127.0.0.1 (not 0.0.0.0)
- [ ] CORS configured correctly
- [ ] No `dangerouslySetInnerHTML` in React components
- [ ] FTS5 queries sanitized
- [ ] Input validation on all API endpoints

### Data Security

- [ ] Database encryption enabled (if needed)
- [ ] Capture directory permissions restricted
- [ ] Secure deletion configured (if needed)
- [ ] Regular backups (encrypted)
- [ ] Retention policy tested

### Deployment Security

- [ ] Running as limited user (not admin)
- [ ] File permissions set correctly
- [ ] No network exposure (localhost only)
- [ ] Logs reviewed for suspicious activity
- [ ] Dependencies up to date (`cargo audit`)

---

## 🚨 Incident Response

### Data Breach

**If database/captures compromised**:

1. **Immediate Actions**:
   - Stop the service
   - Rotate API keys (if added in future)
   - Review access logs

2. **Assessment**:
   - Determine scope (what data accessed?)
   - Check for unauthorized API calls
   - Review system logs

3. **Recovery**:
   - Delete compromised database
   - Change Windows password
   - Re-encrypt captures
   - Review excluded apps list

### Suspicious Activity

**Signs**:
- Unexpected captures from excluded apps
- Database queries during locked sessions
- High CPU/disk usage when idle
- Network connections to non-localhost

**Investigation**:
```bash
# Check running processes
Get-Process | Where-Object {$_.ProcessName -like "*screen*"}

# Review API logs (if logging enabled)
tail -f screensearch.log | grep "ERROR\|WARN"

# Check open files
handle screensearch.db
```

---

## 📚 Security Resources

### Code References

| Component | File | Lines | Description |
|-----------|------|-------|-------------|
| **Query Sanitization** | `screen-db/src/queries.rs` | 450-485 | FTS5 query escaping |
| **App Exclusion** | `screen-capture/src/capture.rs` | 85-120 | Excluded apps check |
| **CORS** | `screen-api/src/routes.rs` | 45-70 | CORS middleware |
| **Input Validation** | `screen-api/src/handlers/` | Various | API validation |
| **XSS Prevention** | `screen-ui/src/components/` | Various | Safe React rendering |

### External Resources

- [SQLite Security](https://www.sqlite.org/security.html)
- [Axum Security Best Practices](https://docs.rs/axum/latest/axum/#security)
- [OWASP Top 10](https://owasp.org/Top10/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)

---

## ⚙️ Configuration Example

**Secure `config.toml`**:

```toml
[capture]
interval_ms = 3000
enable_frame_diff = true

[privacy]
excluded_apps = [
    "1Password",
    "KeePass",
    "Bitwarden",
    "Authy",
    "Google Authenticator",
    "Banking App",
    "Health App"
]
pause_on_lock = true

[database]
path = "screensearch.db"
enable_wal = true
retention_days = 30  # Auto-delete old data

[api]
host = "127.0.0.1"  # Localhost only
port = 3131
```

---

## 🔐 Future Security Enhancements

### Planned

- [ ] Optional database encryption (SQLCipher)
- [ ] API authentication (JWT tokens)
- [ ] Audit logging (all API calls)
- [ ] Screenshot redaction (blur sensitive regions)
- [ ] Browser extension for privacy mode
- [ ] Secure export (encrypted archives)

### Under Consideration

- [ ] End-to-end encryption for cloud sync
- [ ] Hardware security module (HSM) support
- [ ] Two-factor authentication for settings
- [ ] Biometric authentication integration

---

**Security Version**: 1.0.0
**Last Updated**: 2025-12-11
**Security Contact**: Open an issue on GitHub

---

**Found a security vulnerability?**
Please report privately via GitHub Security Advisories or email (if configured).
