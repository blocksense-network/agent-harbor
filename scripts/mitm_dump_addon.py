# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import os
import json
import time
import zlib
import gzip
from pathlib import Path
from mitmproxy import http

# Optional decoders (loaded lazily if present)
try:
    import brotli as _brotli
except Exception:
    _brotli = None

try:
    import zstandard as _zstd
except Exception:
    _zstd = None

OUTDIR = Path(os.environ.get("MITM_DUMP_OUTDIR", "./.mitmproxy/dumps"))
DECODE = os.environ.get("MITM_DUMP_DECODE", "1") not in ("0", "false", "False")


# ---------- helpers ----------

def _first_header(headers, name: str):
    # preserve case-insensitive lookup but keep duplicates when dumping headers elsewhere
    try:
        return headers.get(name)
    except Exception:
        # mitmproxy headers act like a MultiDict; .get(name) is fine, but be defensive
        for k, v in headers.items(True):
            if k.lower() == name.lower():
                return v
    return None


def _split_encodings(v: str):
    # e.g. "gzip, br" -> ["gzip", "br"], trimmed & lowercased
    if not v:
        return []
    return [p.strip().lower() for p in v.split(",") if p.strip()]


def _decode_once(encoding: str, data: bytes):
    if not data:
        return data
    if encoding == "gzip":
        return gzip.decompress(data)
    if encoding == "deflate":
        # try raw DEFLATE, then zlib-wrapped
        try:
            return zlib.decompress(data, wbits=-zlib.MAX_WBITS)
        except zlib.error:
            return zlib.decompress(data)
    if encoding == "br":
        if _brotli is None:
            raise RuntimeError("brotli module not available")
        return _brotli.decompress(data)
    if encoding == "zstd":
        if _zstd is None:
            raise RuntimeError("zstandard module not available")
        d = _zstd.ZstdDecompressor()
        return d.decompress(data)
    # Unknown/identity
    if encoding in ("identity",):
        return data
    raise RuntimeError(f"unknown content-encoding '{encoding}'")


def _decode_chain(encodings, data: bytes):
    """
    Content-Encoding is applied in-order by the sender; to decode we reverse it.
    Returns (decoded_bytes, applied_encodings, errors)
    """
    applied = []
    errors = []
    b = data
    # decode in reverse order
    for enc in reversed(encodings):
        try:
            b = _decode_once(enc, b)
            applied.append(enc)
        except Exception as e:
            errors.append(f"{enc}: {e}")
            # stop if we cannot decode further; keep partially decoded b
            break
    return b, applied, errors


def _looks_textual(content_type: str | None):
    if not content_type:
        return False
    ct = content_type.split(";")[0].strip().lower()
    if ct.startswith("text/"):
        return True
    return ct in (
        "application/json",
        "application/ld+json",
        "application/javascript",
        "application/xml",
        "application/xhtml+xml",
        "application/x-www-form-urlencoded",
    )


def _charset_from_content_type(content_type: str | None):
    if not content_type:
        return "utf-8"
    parts = [p.strip() for p in content_type.split(";")]
    for p in parts[1:]:
        if p.lower().startswith("charset="):
            return p.split("=", 1)[1].strip()
    return "utf-8"


def _dump_binary(path: Path, data: bytes):
    if data is None:
        return
    path.write_bytes(data)


def _dump_textish(base: Path, decoded: bytes, content_type: str | None):
    # pretty print JSON, else write .txt with best-effort decoding
    try:
        ct_main = (content_type or "").split(";")[0].strip().lower()
        if ct_main in ("application/json", "application/ld+json"):
            try:
                obj = json.loads(decoded.decode(_charset_from_content_type(content_type), errors="replace"))
            except Exception:
                # sometimes payload is JSON but charset differs or partial; fall back to bytes parse
                obj = json.loads(decoded.decode("utf-8", errors="replace"))
            (base.with_suffix(".json")).write_text(json.dumps(obj, indent=2, ensure_ascii=False))
            return
    except Exception:
        pass
    # generic text
    txt = decoded.decode(_charset_from_content_type(content_type), errors="replace")
    (base.with_suffix(".txt")).write_text(txt)


def _write_request(flow: http.HTTPFlow, d: Path):
    req = flow.request
    d.mkdir(parents=True, exist_ok=True)

    # metadata
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

    # dump raw request body
    raw = req.raw_content or b""
    _dump_binary(d / "request.body", raw)

    # try decoding (requests are rarely encoded, but support it)
    encs = _split_encodings(_first_header(req.headers, "content-encoding"))
    decoded = raw
    applied = []
    errors = []
    if DECODE and encs:
        decoded, applied, errors = _decode_chain(encs, raw)
    meta["content_encoding"] = encs
    meta["decoded"] = bool(applied)
    if applied:
        _dump_binary(d / "request.body.decoded", decoded)
        # write text/json if looks textual
        if _looks_textual(_first_header(req.headers, "content-type")):
            _dump_textish(d / "request.body.decoded", decoded, _first_header(req.headers, "content-type"))
    if errors:
        meta["decode_errors"] = errors

    (d / "request.json").write_text(json.dumps(meta, indent=2))


def _write_response(flow: http.HTTPFlow, d: Path):
    resp = flow.response
    d.mkdir(parents=True, exist_ok=True)

    # metadata
    meta = {
        "timestamp_start": resp.timestamp_start,
        "timestamp_end": resp.timestamp_end,
        "status_code": resp.status_code,
        "reason": resp.reason,
        "http_version": resp.http_version,
        "headers": list(resp.headers.items(True)),
        "content_length": len(resp.raw_content) if resp.raw_content else 0,
    }

    # raw response body
    raw = resp.raw_content or b""
    _dump_binary(d / "response.body", raw)

    # decode according to content-encoding
    encs = _split_encodings(_first_header(resp.headers, "content-encoding"))
    decoded = raw
    applied = []
    errors = []
    if DECODE and encs:
        decoded, applied, errors = _decode_chain(encs, raw)

    meta["content_encoding"] = encs
    meta["decoded"] = bool(applied)
    if applied:
        _dump_binary(d / "response.body.decoded", decoded)
        # write a friendly .json/.txt if it's likely text
        if _looks_textual(_first_header(resp.headers, "content-type")):
            _dump_textish(d / "response.body.decoded", decoded, _first_header(resp.headers, "content-type"))
    if errors:
        meta["decode_errors"] = errors

    (d / "response.json").write_text(json.dumps(meta, indent=2))


# ---------- mitmproxy hooks ----------

def request(flow: http.HTTPFlow):
    OUTDIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%d_%H%M%S", time.localtime())
    d = OUTDIR / f"{ts}_{int(time.time()*1000)}"
    d.mkdir(parents=True, exist_ok=True)
    flow.metadata["dump_dir"] = str(d)
    _write_request(flow, d)


def response(flow: http.HTTPFlow):
    d = Path(flow.metadata.get("dump_dir", OUTDIR / f"{time.strftime('%Y%m%d_%H%M%S', time.localtime())}_{int(time.time()*1000)}"))
    d.mkdir(parents=True, exist_ok=True)
    _write_response(flow, d)
