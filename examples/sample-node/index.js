const http = require('http');
const PORT = process.env.PORT || 3000;
let started = Date.now();
let counter = 0;

const server = http.createServer((req, res) => {
  if (req.url === '/ready') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ status: 'ok', uptime_ms: Date.now() - started }));
    return;
  }
  if (req.url === '/' || req.url === '/healthz') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ message: 'hello', counter: ++counter, time: new Date().toISOString() }));
    return;
  }
  res.writeHead(404, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ error: 'not_found' }));
});

server.listen(PORT, () => {
  console.log(`sample-node listening on :${PORT}`);
});
