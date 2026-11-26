#!/usr/bin/env sh
# Reports what chunkfetti needs on this computer. Changes nothing, downloads nothing.
set -u
ok() { printf '  ok      %s\n' "$1"; }
miss() { printf '  missing %s\n' "$1"; }

echo "system:  $(uname -s) $(uname -m)"
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo "?")
echo "cores:   $cores"
echo

echo "builder (one of these is enough):"
if command -v chunkfetti >/dev/null 2>&1; then ok "chunkfetti on PATH: $(chunkfetti version)"
elif [ -x ./bin/chunkfetti ]; then ok "./bin/chunkfetti: $(./bin/chunkfetti version)"
else miss "chunkfetti binary (download it from the latest release, or build from source)"; fi
if command -v cargo >/dev/null 2>&1; then ok "Rust: $(cargo --version)"
else miss "Rust (only needed to build from source)"; fi
echo

echo "preview:"
if command -v node >/dev/null 2>&1; then
  major=$(node -p 'process.versions.node.split(".")[0]')
  if [ "$major" -ge 20 ]; then ok "Node.js $(node --version)"; else miss "Node.js 20 or newer (found $(node --version))"; fi
else miss "Node.js 20 or newer (only needed for the local preview)"; fi
