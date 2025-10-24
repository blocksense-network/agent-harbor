# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import json
import time
from pathlib import Path
from mitmproxy import http

OUTDIR = Path(os.environ.get("MITM_DUMP_OUTDIR", "./.mitmproxy/dumps"))

def request(flow: http.HTTPFlow):
    OUTDIR.mkdir(parents=True, exist_ok=True)
    d = OUTDIR / f"{int(time.time()*1000)}-{flow.id}"
    d.mkdir(parents=True, exist_ok=True)
    flow.metadata["dump_dir"] = str(d)

    req = flow.request
    meta = {
        "timestamp_start": req.timestamp_start,
        "method": req.method,
        "scheme": req.scheme,
        "host": req.host,
        "port": req.port,
        "path": req.path,
        "http_version": req.http_version,
        "headers": list(req.headers.items(True)),
        "content_length": len(req.raw_content) if req.raw_content else 0,
        "pretty_url": req.pretty_url,
    }
    (d / "request.json").write_text(json.dumps(meta, indent=2))
    if req.raw_content:
        (d / "request.body").write_bytes(req.raw_content)

def response(flow: http.HTTPFlow):
    d = Path(flow.metadata.get("dump_dir", OUTDIR / f"{int(time.time()*1000)}-{flow.id}"))
    d.mkdir(parents=True, exist_ok=True)

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
    (d / "response.json").write_text(json.dumps(meta, indent=2))
    if resp.raw_content:
        (d / "response.body").write_bytes(resp.raw_content)
