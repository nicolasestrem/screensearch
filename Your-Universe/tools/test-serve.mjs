// Gate for the static server: one malformed request must not be able to stop it.
//
// A percent escape that decodeURIComponent cannot parse (GET /%ZZ) used to throw outside the
// request handler's error boundary, so the async callback rejected, nothing caught it and the
// process exited with URIError while the client got ECONNRESET. This asserts the 400 instead of
// the crash, and that the same server still answers a normal request afterwards.
import { spawn } from 'node:child_process';
import { connect, createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');

function freePort() {
  return new Promise(function (done, fail) {
    const probe = createServer();
    probe.once('error', fail);
    probe.listen(0, '127.0.0.1', function () {
      const port = probe.address().port;
      probe.close(function () {
        done(port);
      });
    });
  });
}

// Raw socket request, so the malformed target reaches the parser exactly as typed.
function rawRequest(port, target) {
  return new Promise(function (done) {
    const socket = connect(port, '127.0.0.1');
    socket.setEncoding('utf8');
    let text = '';
    const finish = function (note) {
      socket.destroy();
      const match = text.match(/^HTTP\/1\.1 (\d{3})/);
      done({ status: match ? Number(match[1]) : 0, body: text, note: note || '' });
    };
    socket.on('connect', function () {
      socket.write('GET ' + target + ' HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n');
    });
    socket.on('data', function (chunk) {
      text += chunk;
    });
    socket.on('end', function () {
      finish('');
    });
    socket.on('error', function (err) {
      finish(err.code || String(err));
    });
    socket.setTimeout(5000, function () {
      finish('timeout');
    });
  });
}

function sleep(ms) {
  return new Promise(function (done) {
    setTimeout(done, ms);
  });
}

const failures = [];
function check(ok, label) {
  console.log((ok ? 'PASS  ' : 'FAIL  ') + label);
  if (!ok) failures.push(label);
}

const port = await freePort();
const child = spawn(process.execPath, [join(here, 'serve.mjs'), root, String(port)], {
  stdio: ['ignore', 'pipe', 'pipe'],
});
let output = '';
child.stdout.on('data', function (chunk) {
  output += chunk;
});
child.stderr.on('data', function (chunk) {
  output += chunk;
});

const startedAt = Date.now();
while (output.indexOf('serving ') < 0 && child.exitCode === null && Date.now() - startedAt < 8000) {
  await sleep(50);
}
check(output.indexOf('serving ') >= 0, 'the server announces its root and port');
check(child.exitCode === null, 'the server is running before any request');

const normal = await rawRequest(port, '/index.html');
check(normal.status === 200, 'GET /index.html answers 200 (got ' + normal.status + ')');
check(normal.body.length > 100000, 'GET /index.html returns the built page (' + normal.body.length + ' bytes)');

const malformed = await rawRequest(port, '/%ZZ');
check(
  malformed.status === 400,
  'GET /%ZZ answers 400 instead of dropping the connection (got ' +
    malformed.status +
    (malformed.note ? ', ' + malformed.note : '') +
    ')'
);

await sleep(200);
check(child.exitCode === null, 'the server survives the malformed request (exit code ' + child.exitCode + ')');

const after = await rawRequest(port, '/index.html');
check(after.status === 200, 'GET /index.html still answers 200 afterwards (got ' + after.status + ')');

child.kill();
await sleep(100);
if (failures.length > 0) {
  console.log('');
  console.log('server log:');
  console.log(output.trim());
  console.log('');
  console.log('FAILED: ' + failures.length + ' check(s) failed');
  process.exitCode = 1;
} else {
  console.log('');
  console.log('OK: the server answers a malformed escape with 400 and keeps serving');
}
