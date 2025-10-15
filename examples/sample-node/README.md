# Aether Sample Node App

This directory is auto-generated (or can be regenerated) by `./dev.sh deploy-sample <app>` when no path is provided.

Contents:
- `index.js` simple HTTP server exposing JSON with uptime & counter.
- `package.json` minimal metadata.

You can edit `index.js` and run:
```
./dev.sh hot-upload <app> examples/sample-node
./dev.sh hot-patch <app> <new-digest>
```
to trigger a live reload in the running Kubernetes pod (sidecar fetcher updates shared volume).

Regenerate (will not overwrite if directory already exists):
```
rm -rf examples/sample-node
./dev.sh deploy-sample demo
```
