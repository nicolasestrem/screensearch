// Zero-dependency static server. Serving is how the deliverable runs: the city loads its
// glTF packs from assets/ with fetch, which a local-file page cannot do, so a local-file
// open falls back to the procedural build and a served open shows the modelled one.
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { extname, join, normalize, resolve } from 'node:path';

const root = resolve(process.argv[2] ?? '.');
const port = Number(process.argv[3] ?? 8731);
const types = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.glb': 'model/gltf-binary',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
};

const server = createServer(async (req, res) => {
  const url = new URL(req.url ?? '/', 'http://localhost');
  const rel = decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname);
  const file = join(root, normalize(rel).replace(/^([/\\])+/, ''));
  if (rel === '/favicon.ico') {
    // index.html is a single file with no icon link on purpose, so the browser always asks for
    // this and the 404 would sit in the console of every verification run.
    res.writeHead(204, { 'cache-control': 'no-store' }).end();
    return;
  }
  if (!file.startsWith(root)) {
    res.writeHead(403).end('forbidden');
    return;
  }
  try {
    const info = await stat(file);
    if (!info.isFile()) throw new Error('not a file');
    const body = await readFile(file);
    const extension = extname(file).toLowerCase();
    const inAssets = rel.indexOf('/assets/') === 0;
    const headers = {
      'content-type': types[extension] ?? 'application/octet-stream',
      'content-length': body.length,
      'cache-control': 'no-store',
    };
    if (rel === '/index.html') {
      headers['cache-control'] = 'no-cache';
    } else if (inAssets && extension === '.glb') {
      // The pack GLBs are content-stable; the service worker versions its cache.
      headers['cache-control'] = 'public, max-age=31536000';
    }
    if (rel === '/assets/sw.js') {
      // The worker lives in /assets/ but must control the site root scope.
      headers['service-worker-allowed'] = '/';
    }
    res.writeHead(200, headers);
    res.end(body);
  } catch {
    res.writeHead(404, { 'content-type': 'text/plain' }).end('not found');
  }
});

server.listen(port, '127.0.0.1', () => {
  console.log(`serving ${root} at http://127.0.0.1:${port}/`);
});
