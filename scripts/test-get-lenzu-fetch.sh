#!/usr/bin/env bash
# Regression test for the fetch() helper in get-lenzu.sh.  ref #64
#
# The bug: the download guard was an existence-only check, so an interrupted
# transfer left a truncated file that the next run reported as "already
# downloaded" and symlinked as though it were whole.  These cases pin the four
# behaviours the fix depends on.
#
#   1. clean download       -> dest written, no .part left behind
#   2. mid-transfer abort   -> non-zero exit, NO dest file, no .part left behind
#   3. resume from .part    -> appends the remainder, result is byte-identical
#   4. dest already present -> no download attempted, returns 0
#
# Case 2 is the regression guard.  It needs a server that dies mid-body: a
# failure detected before any bytes are written (a 404, or a missing file://
# target) does NOT create the destination, so both the old and new code pass
# such a check and it proves nothing.  The stub below advertises a
# Content-Length it never finishes sending, which is what an interrupted
# download actually looks like.
#
# fetch() is extracted from get-lenzu.sh at runtime rather than duplicated, so
# this test cannot drift from the code it guards.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
PORT="${LENZU_TEST_PORT:-18099}"
SERVER_PID=""

cleanup() {
    [[ -n "$SERVER_PID" ]] && kill "$SERVER_PID" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

# Pull the real fetch() out of the script under test.
eval "$(sed -n '/^fetch() {/,/^}/p' "$REPO_ROOT/scripts/get-lenzu.sh")"
declare -F fetch >/dev/null || { echo "FAIL: could not extract fetch() from get-lenzu.sh"; exit 1; }

pass=0
fail=0
check() {
    local desc="$1" cond="$2"
    if eval "$cond"; then
        echo "  ok   $desc"
        pass=$((pass + 1))
    else
        echo "  FAIL $desc"
        fail=$((fail + 1))
    fi
}

run_fetch() {
    local url="$1" dest="$2" rc=0
    fetch "$url" "$dest" >/dev/null 2>&1 || rc=$?
    return $rc
}

# 5 KB of deterministic content, so the resume case can be byte-compared.
head -c 5000 /dev/zero | tr '\0' 'A' > "$WORK/src.bin"
SRC_BYTES=$(stat -c%s "$WORK/src.bin")

echo "==> 1. clean download"
dest="$WORK/clean.bin"
fetch "file://$WORK/src.bin" "$dest" >/dev/null
check "dest exists"                   "[[ -f $dest ]]"
check "dest is full size ($SRC_BYTES)" "[[ \$(stat -c%s $dest) -eq $SRC_BYTES ]]"
check "no .part left behind"          "[[ ! -e ${dest}.part ]]"

echo "==> 2. mid-transfer abort must not leave a dest file"
if ! command -v node >/dev/null; then
    echo "  SKIP  node not available; cannot stage an interrupted transfer"
else
    cat > "$WORK/stub.js" <<EOF
const http = require('http');
const TOTAL = ${SRC_BYTES}, SEND = 2000;
http.createServer((req, res) => {
    res.writeHead(200, { 'Content-Length': String(TOTAL) });
    res.write(Buffer.alloc(SEND, 0x42));
    setTimeout(() => res.socket.destroy(), 50);
}).listen(${PORT}, '127.0.0.1');
EOF
    node "$WORK/stub.js" &
    SERVER_PID=$!
    for _ in $(seq 1 50); do
        nc -z 127.0.0.1 "$PORT" 2>/dev/null && break
        sleep 0.1
    done

    dest2="$WORK/aborted.bin"
    if run_fetch "http://127.0.0.1:${PORT}/src.bin" "$dest2"; then rc=0; else rc=$?; fi
    check "returns non-zero"        "[[ $rc -ne 0 ]]"
    check "NO dest file created"    "[[ ! -e $dest2 ]]"
    check "no .part left behind"    "[[ ! -e ${dest2}.part ]]"

    kill "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
fi

echo "==> 3. resume from an existing .part"
dest3="$WORK/resumed.bin"
head -c 1000 "$WORK/src.bin" > "${dest3}.part"
fetch "file://$WORK/src.bin" "$dest3" >/dev/null
check "dest is full size"           "[[ \$(stat -c%s $dest3) -eq $SRC_BYTES ]]"
check "byte-identical to source"    "cmp -s $WORK/src.bin $dest3"
check "no .part left behind"        "[[ ! -e ${dest3}.part ]]"

echo "==> 4. dest already present is left alone"
dest4="$WORK/present.bin"
cp "$WORK/src.bin" "$dest4"
if run_fetch "file://$WORK/does-not-exist.bin" "$dest4"; then rc=0; else rc=$?; fi
check "returns 0 without downloading" "[[ $rc -eq 0 ]]"
check "dest untouched"                "cmp -s $WORK/src.bin $dest4"

echo ""
echo "passed: $pass  failed: $fail"
[[ $fail -eq 0 ]]
