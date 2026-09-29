#!/bin/sh
# scripts/install.sh — install the my-aidlc command.
#
# Usage:
#   ./scripts/install.sh [--from <dir>] [--prefix <dir>] [--bin-dir <dir>]
#                        [--version <x.y.z>] [--uninstall] [--no-completion]
#                        [--quiet]
#
# Environment:
#   MY_AIDLC_INSTALL_ROOT  install root (default: $XDG_DATA_HOME/my-aidlc)
#   MY_AIDLC_BIN_DIR       command directory (default: $HOME/.local/bin)
#   MY_AIDLC_RELEASE_BASE_URL  release base URL for online installs
#
# The installer copies the runtime (core/, harness/, scripts/, package.json)
# into the install root and writes a small launcher that sets MY_AIDLC_HOME.
# Node.js 20+ is required.

set -eu

REPO=${MY_AIDLC_REPOSITORY:-doitsu2014/my-aidlc}
RELEASE_BASE=${MY_AIDLC_RELEASE_BASE_URL:-https://github.com/$REPO/releases}
INSTALL_ROOT=${MY_AIDLC_INSTALL_ROOT:-"${XDG_DATA_HOME:-$HOME/.local/share}/my-aidlc"}
BIN_DIR=${MY_AIDLC_BIN_DIR:-"$HOME/.local/bin"}
FROM=
VERSION=
UNINSTALL=0
QUIET=0
COMPLETION=1

usage() {
  cat <<EOF
Usage: install.sh [--from <dir>] [--prefix <dir>] [--bin-dir <dir>]
                  [--version <x.y.z>] [--uninstall] [--no-completion] [--quiet]
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --from) FROM=${2:?--from requires a directory}; shift 2 ;;
    --prefix) INSTALL_ROOT=$2; shift 2 ;;
    --bin-dir) BIN_DIR=$2; shift 2 ;;
    --version) VERSION=$2; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    --no-completion) COMPLETION=0; shift ;;
    --quiet) QUIET=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

say() { [ "$QUIET" -eq 1 ] || printf '%s\n' "$*"; }
die() { printf 'ERROR %s\n' "$*" >&2; exit 1; }

detect_shell() {
  case "${SHELL:-}" in
    *zsh*) printf 'zsh' ;;
    *bash*) printf 'bash' ;;
    *fish*) printf 'fish' ;;
    *pwsh*|*powershell*) printf 'powershell' ;;
    *) printf '' ;;
  esac
}

command -v node >/dev/null 2>&1 || die "Node.js 20+ is required but 'node' was not found."

if [ "$UNINSTALL" -eq 1 ]; then
  shell_name=$(detect_shell)
  if [ -n "$shell_name" ] && [ -f "$INSTALL_ROOT/core/tools/my-aidlc.mjs" ]; then
    MY_AIDLC_HOME="$INSTALL_ROOT" node "$INSTALL_ROOT/core/tools/my-aidlc.mjs" \
      completion uninstall --shell "$shell_name" >/dev/null 2>&1 || true
  fi
  rm -f "$BIN_DIR/my-aidlc"
  rm -rf "$INSTALL_ROOT"
  say "Removed my-aidlc from $INSTALL_ROOT and $BIN_DIR/my-aidlc"
  exit 0
fi

# Resolve the source tree: --from, else the repository containing this script.
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
if [ -z "$FROM" ]; then
  if [ -f "$script_dir/../package.json" ] && [ -d "$script_dir/../core" ]; then
    FROM=$(CDPATH='' cd -- "$script_dir/.." && pwd -P)
  else
    FROM=""
  fi
fi

if [ -z "$FROM" ]; then
  [ -n "$VERSION" ] || die "no local source found; pass --from <dir> or --version <x.y.z>"
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/my-aidlc.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT HUP INT TERM
  url="$RELEASE_BASE/download/v$VERSION/my-aidlc-runtime-$VERSION.tar.gz"
  say "Downloading $url"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$tmp/runtime.tar.gz" || die "download failed"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$tmp/runtime.tar.gz" || die "download failed"
  else
    die "curl or wget is required for an online install"
  fi
  mkdir -p "$tmp/src"
  tar -xzf "$tmp/runtime.tar.gz" -C "$tmp/src" || die "failed to extract release archive"
  FROM="$tmp/src"
fi

[ -f "$FROM/package.json" ] || die "$FROM is not a my-aidlc source tree (no package.json)"
[ -d "$FROM/core" ] || die "$FROM is missing core/"

say "Installing my-aidlc from $FROM"
rm -rf "$INSTALL_ROOT"
mkdir -p "$INSTALL_ROOT"
for item in core harness scripts package.json README.md LICENSE; do
  [ -e "$FROM/$item" ] || continue
  cp -R "$FROM/$item" "$INSTALL_ROOT/"
done

mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/my-aidlc" <<EOF
#!/bin/sh
# my-aidlc launcher (installer-owned)
MY_AIDLC_HOME="$INSTALL_ROOT"
export MY_AIDLC_HOME
exec node "\$MY_AIDLC_HOME/core/tools/my-aidlc.mjs" "\$@"
EOF
chmod 755 "$BIN_DIR/my-aidlc"

installed_version=$(node -e "process.stdout.write(require('$INSTALL_ROOT/package.json').version)" 2>/dev/null || echo "0.1.0")
say "PASS installed my-aidlc $installed_version"

if [ "$COMPLETION" -eq 1 ]; then
  shell_name=$(detect_shell)
  if [ -n "$shell_name" ]; then
    if MY_AIDLC_HOME="$INSTALL_ROOT" node "$INSTALL_ROOT/core/tools/my-aidlc.mjs" \
      completion install --shell "$shell_name" >/dev/null 2>&1; then
      say "PASS installed $shell_name completion (restart your shell to activate)"
    else
      say "note: $shell_name completion was not installed; run 'my-aidlc completion install'"
    fi
  fi
fi

case ":$PATH:" in
  *":$BIN_DIR:"*) say "Next: my-aidlc config --harness pi" ;;
  *)
    say "Add my-aidlc to PATH for this shell:"
    say "  export PATH=\"$BIN_DIR:\$PATH\""
    say "Then run: my-aidlc config --harness pi"
    ;;
esac
