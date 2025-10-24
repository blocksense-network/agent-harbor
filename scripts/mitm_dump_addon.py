# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os, json, time
from mitmproxy import http

OUTDIR = os.environ.get("MITM_DUMP_OUTDIR", "./.mitmproxy/dumps")

def _mk_flow_dir(flow):
  # Stable dir per flow; created on request hook
  return flow.metadata.get("dump_dir")

def request(flow: http.HTTPFlow):
  os.makedirs(OUTDIR, exist_ok=True)
  # Subdir: <epoch_ms>-<flow_id>  (helps chronological browsing)
  prefix = f"{int(time.time()*1000)}-{flow.id}"
  d = os.path.join(OUTDIR, prefix)
  os.makedirs(d, exist_ok=True)
  flow.metadata["dump_dir"] = d

  req = flow.request
  meta = {
    "timestamp_start": req.timestamp_start,
    "method": req.method,
    "scheme": req.scheme,
    "host": req.host,
    "port": req.port,
    "path": req.path,
    "http_version": req.http_version,
    "headers": list(req.headers.items(True)),  # preserve case/dupes
    "content_length": len(req.raw_content) if req.raw_content else 0,
    "pretty_url": req.pretty_url,
  }
  with open(os.path.join(d, "request.json"), "w") as f:
    json.dump(meta, f, indent=2)
  if req.raw_content:
    with open(os.path.join(d, "request.body"), "wb") as f:
      f.write(req.raw_content)

def response(flow: http.HTTPFlow):
  d = _mk_flow_dir(flow)
  if not d:
    # In case response fires first (rare), make dir now.
    d = os.path.join(OUTDIR, f"{int(time.time()*1000)}-{flow.id}")
    os.makedirs(d, exist_ok=True)
    flow.metadata["dump_dir"] = d

  resp = flow.response
  meta = {
    "timestamp_start": resp.timestamp_start,
    "timestamp_end": resp.timestamp_end,
    "status_code": resp.status_code,
    "reason": resp.reason,
    "http_version": resp.http_version,
    "headers": list(resp.headers.items(True)),
    "content_length": len(resp.raw_content) if resp.raw_content else 0,
  }
  with open(os.path.join(d, "response.json"), "w") as f:
    json.dump(meta, f, indent=2)
  if resp.raw_content:
    with open(os.path.join(d, "response.body"), "wb") as f:
      f.write(resp.raw_content)
