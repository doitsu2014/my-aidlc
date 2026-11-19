#!/bin/sh
# Install my-aidlc from a local source tree. No network access is used.
set -eu

usage() {
  printf '%s\n' 'Usage: install.sh [--from SOURCE_DIR] [--prefix PREFIX]' \
    '  --from SOURCE_DIR  Local project source directory (default: script parent)' \
    '  --prefix PREFIX   Installation prefix (default: ~/.local)' \
    '  -h, --help        Show this help'
}

fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

source_dir=
if [ -n "${MY_AIDLC_SOURCE:-}" ]; then source_dir=$MY_AIDLC_SOURCE; fi
prefix=${MY_AIDLC_PREFIX:-"${HOME:?HOME must be set}/.local"}

while [ "$#" -gt 0 ]; do
  case $1 in
    --from)
      [ "$#" -ge 2 ] || fail '--from requires a directory'
      source_dir=$2; shift 2 ;;
    --prefix)
      [ "$#" -ge 2 ] || fail '--prefix requires a directory'
      prefix=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
[ -n "$source_dir" ] || source_dir=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
[ -d "$source_dir" ] || fail "source directory not found: $source_dir"
source_dir=$(CDPATH= cd -- "$source_dir" && pwd -P)
[ -f "$source_dir/package.json" ] || fail 'source is missing package.json'
[ -f "$source_dir/core/tools/my-aidlc.mjs" ] || fail 'source is missing core/tools/my-aidlc.mjs'
[ -f "$source_dir/core/tools/lib.mjs" ] || fail 'source is missing core/tools/lib.mjs'
[ -f "$source_dir/core/data/phase-graph.json" ] || fail 'source is missing core/data/phase-graph.json'
[ -d "$source_dir/core" ] && [ -d "$source_dir/harness" ] || fail 'source is missing core/ or harness/'

case $prefix in
  /*) ;;
  *) prefix=$(pwd -P)/$prefix ;;
esac
mkdir -p "$prefix"
prefix=$(CDPATH= cd -- "$prefix" && pwd -P)
install_root=$prefix/lib/my-aidlc
bin_dir=$prefix/bin
launcher=$bin_dir/my-aidlc

# Never replace an existing installation or launcher without explicit removal.
[ ! -e "$install_root" ] && [ ! -L "$install_root" ] || fail "installation already exists: $install_root (remove it manually before reinstalling)"
[ ! -e "$launcher" ] && [ ! -L "$launcher" ] || fail "launcher already exists: $launcher (remove it manually before reinstalling)"

mkdir -p "$bin_dir"
created_root=0
created_launcher=0
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    [ "$created_launcher" -eq 0 ] || rm -f "$launcher"
    [ "$created_root" -eq 0 ] || rm -rf "$install_root"
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$install_root"
created_root=1
# Copy only the runtime tree, never git metadata or the caller's workspace.
cp -R "$source_dir/core" "$install_root/core"
cp -R "$source_dir/harness" "$install_root/harness"
cp "$source_dir/package.json" "$install_root/package.json"

# Resolve Node at invocation time from the launcher's location. Keeping the
# installation path as runtime data avoids embedding/quoting it as shell code.
tmp_launcher=$bin_dir/.my-aidlc.tmp.$$
(umask 077; printf '%s\n' '#!/bin/sh' 'set -eu' 'script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)' 'exec node "$script_dir/../lib/my-aidlc/core/tools/my-aidlc.mjs" "$@"' > "$tmp_launcher")
chmod 755 "$tmp_launcher"
# Hard-link publication is atomic and fails rather than replacing an existing path.
ln "$tmp_launcher" "$launcher" || fail "launcher appeared during install: $launcher"
created_launcher=1
rm -f "$tmp_launcher"
trap - EXIT HUP INT TERM
printf 'Installed my-aidlc to %s\n' "$launcher"
printf '%s\n' 'Add the prefix bin directory to PATH if needed.'
