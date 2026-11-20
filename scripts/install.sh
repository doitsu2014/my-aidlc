#!/bin/sh
# Install, upgrade or uninstall dtech-aidlc from a local source tree. No network access is used.
set -eu

usage() {
  printf '%s\n' 'Usage: install.sh [--from SOURCE_DIR] [--prefix PREFIX] [--upgrade | --uninstall]' \
    '  --from SOURCE_DIR  Local project source directory (default: script parent)' \
    '  --prefix PREFIX   Installation prefix (default: ~/.local)' \
    '  --upgrade         Replace an existing installation (or repair a partial one)' \
    '  --uninstall       Remove the library, launcher and zsh completion' \
    '  -h, --help        Show this help'
}

fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

source_dir=
if [ -n "${DTECH_AIDLC_SOURCE:-}" ]; then source_dir=$DTECH_AIDLC_SOURCE; fi
prefix=${DTECH_AIDLC_PREFIX:-"${HOME:?HOME must be set}/.local"}
upgrade=0
uninstall=0

while [ "$#" -gt 0 ]; do
  case $1 in
    --from)
      [ "$#" -ge 2 ] || fail '--from requires a directory'
      source_dir=$2; shift 2 ;;
    --prefix)
      [ "$#" -ge 2 ] || fail '--prefix requires a directory'
      prefix=$2; shift 2 ;;
    --upgrade) upgrade=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done
[ "$upgrade" -eq 0 ] || [ "$uninstall" -eq 0 ] || fail '--upgrade and --uninstall cannot be combined'

case $prefix in
  /*) ;;
  *) prefix=$(pwd -P)/$prefix ;;
esac

present() { [ -e "$1" ] || [ -L "$1" ]; }
read_version() {
  [ -f "$1" ] || { printf '%s' unknown; return; }
  v=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -n 1)
  printf '%s' "${v:-unknown}"
}
# Recognition rules: only files that dtech-aidlc itself installed are ever replaced or removed.
lib_is_ours() {
  [ -d "$1" ] && [ ! -L "$1" ] || return 1
  if [ -z "$(ls -A "$1")" ]; then return 0; fi
  [ -f "$1/package.json" ] && [ -f "$1/core/tools/dtech-aidlc.mjs" ] &&
    grep -q '"name"[[:space:]]*:[[:space:]]*"dtech-aidlc"' "$1/package.json"
}
launcher_is_ours() {
  [ -f "$1" ] && [ ! -L "$1" ] && grep -q 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs' "$1"
}
completion_is_ours() {
  [ -f "$1" ] && [ ! -L "$1" ] && [ "$(head -n 1 "$1")" = '#compdef dtech-aidlc' ]
}
# Fail before any change when an existing path is not ours.
check_recognised() {
  if present "$install_root" && ! lib_is_ours "$install_root"; then fail "$install_root exists but is not a dtech-aidlc file; not touching it"; fi
  if present "$launcher" && ! launcher_is_ours "$launcher"; then fail "$launcher exists but is not a dtech-aidlc file; not touching it"; fi
  if present "$completion" && ! completion_is_ours "$completion"; then fail "$completion exists but is not a dtech-aidlc file; not touching it"; fi
}

if [ "$uninstall" -eq 1 ] && [ ! -d "$prefix" ]; then
  printf 'nothing to uninstall under %s\n' "$prefix"
  exit 0
fi
[ "$uninstall" -eq 1 ] || mkdir -p "$prefix"
prefix=$(CDPATH= cd -- "$prefix" && pwd -P)
install_root=$prefix/lib/dtech-aidlc
bin_dir=$prefix/bin
launcher=$bin_dir/dtech-aidlc
completion=$prefix/share/zsh/site-functions/_dtech-aidlc

if [ "$uninstall" -eq 1 ]; then
  check_recognised
  removed=0
  if present "$launcher"; then rm -f "$launcher"; printf 'Removed %s\n' "$launcher"; removed=1; fi
  if present "$completion"; then rm -f "$completion"; printf 'Removed %s\n' "$completion"; removed=1; fi
  if present "$install_root"; then rm -rf "$install_root"; printf 'Removed %s\n' "$install_root"; removed=1; fi
  [ "$removed" -eq 1 ] || printf 'nothing to uninstall under %s\n' "$prefix"
  exit 0
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
[ -n "$source_dir" ] || source_dir=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
[ -d "$source_dir" ] || fail "source directory not found: $source_dir"
source_dir=$(CDPATH= cd -- "$source_dir" && pwd -P)
[ -f "$source_dir/package.json" ] || fail 'source is missing package.json'
[ -f "$source_dir/core/tools/dtech-aidlc.mjs" ] || fail 'source is missing core/tools/dtech-aidlc.mjs'
[ -f "$source_dir/core/tools/lib.mjs" ] || fail 'source is missing core/tools/lib.mjs'
[ -f "$source_dir/core/data/phase-graph.json" ] || fail 'source is missing core/data/phase-graph.json'
[ -f "$source_dir/core/tools/completion.zsh" ] || fail 'source is missing core/tools/completion.zsh'
[ -d "$source_dir/core" ] && [ -d "$source_dir/harness" ] || fail 'source is missing core/ or harness/'

if [ "$upgrade" -eq 1 ]; then
  check_recognised
else
  existing=
  for path in "$install_root" "$launcher" "$completion"; do
    if present "$path"; then existing="$existing${existing:+ }$path"; fi
  done
  [ -z "$existing" ] || fail "already exists: $existing; run install.sh --upgrade to replace them or install.sh --uninstall to remove them"
fi

write_launcher() {
  # Quoted here-document: nothing expands at install time, so the install path is not embedded.
  (umask 077; cat > "$1" <<'LAUNCHER'
#!/bin/sh
set -eu
self=$0
while [ -L "$self" ]; do
  link=$(readlink "$self")
  case $link in /*) self=$link ;; *) self=$(dirname -- "$self")/$link ;; esac
done
script_dir=$(CDPATH= cd -- "$(dirname -- "$self")" && pwd -P)
entry=$script_dir/../lib/dtech-aidlc/core/tools/dtech-aidlc.mjs
if [ ! -f "$entry" ]; then
  printf '%s\n' "dtech-aidlc: installation is incomplete ($entry is missing). Run install.sh --upgrade from a dtech-aidlc checkout." >&2
  exit 1
fi
exec node "$entry" "$@"
LAUNCHER
  )
  chmod 755 "$1"
}
copy_tree() {
  # Copy only the runtime tree, never git metadata or the caller's workspace.
  mkdir -p "$1"
  cp -R "$source_dir/core" "$1/core"
  cp -R "$source_dir/harness" "$1/harness"
  cp "$source_dir/package.json" "$1/package.json"
}
print_installed() {
  printf '%s\n' 'Installed files:' "  $install_root" "  $launcher" "  $completion"
}

mkdir -p "$prefix/lib" "$bin_dir" "$(dirname "$completion")"
tmp_launcher=$bin_dir/.dtech-aidlc.tmp.$$

if [ "$upgrade" -eq 1 ]; then
  stage=$prefix/lib/.dtech-aidlc.new.$$
  old=$prefix/lib/.dtech-aidlc.old.$$
  tmp_completion=$(dirname "$completion")/.dtech-aidlc.tmp.$$
  moved_old=0
  moved_new=0
  cleanup() {
    status=$?
    if [ "$status" -ne 0 ]; then
      [ "$moved_new" -eq 0 ] || rm -rf "$install_root"
      [ "$moved_old" -eq 0 ] || mv "$old" "$install_root"
    fi
    rm -rf "$stage" "$old" "$tmp_launcher" "$tmp_completion"
    exit "$status"
  }
  trap cleanup EXIT HUP INT TERM
  old_version=none
  if present "$install_root"; then old_version=$(read_version "$install_root/package.json"); fi
  copy_tree "$stage"
  if present "$install_root"; then mv "$install_root" "$old"; moved_old=1; fi
  mv "$stage" "$install_root"; moved_new=1
  write_launcher "$tmp_launcher"
  cp "$source_dir/core/tools/completion.zsh" "$tmp_completion"
  mv -f "$tmp_launcher" "$launcher"
  mv -f "$tmp_completion" "$completion"
  trap - EXIT HUP INT TERM
  rm -rf "$old"
  printf 'Upgraded dtech-aidlc %s -> %s\n' "$old_version" "$(read_version "$install_root/package.json")"
  printf '%s\n' 'Workflow state migrates by itself. In each onboarded project run: dtech-aidlc harness refresh (skill files) and dtech-aidlc onboard (PROJECT.md, knowledge manifest and profile).'
  print_installed
  exit 0
fi

created_root=0
created_launcher=0
created_completion=0
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    [ "$created_completion" -eq 0 ] || rm -f "$completion"
    [ "$created_launcher" -eq 0 ] || rm -f "$launcher"
    [ "$created_root" -eq 0 ] || rm -rf "$install_root"
  fi
  rm -f "$tmp_launcher"
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

created_root=1
copy_tree "$install_root"
write_launcher "$tmp_launcher"
# Hard-link publication is atomic and fails rather than replacing an existing path.
ln "$tmp_launcher" "$launcher" || fail "launcher appeared during install: $launcher"
created_launcher=1
rm -f "$tmp_launcher"
cp "$source_dir/core/tools/completion.zsh" "$completion"
created_completion=1
trap - EXIT HUP INT TERM
printf 'Installed dtech-aidlc to %s\n' "$launcher"
printf 'Installed zsh completion to %s\n' "$completion"
print_installed
printf '%s\n' 'Add the prefix bin directory to PATH if needed.'
printf '%s\n' 'For zsh, add the site-functions directory to fpath before compinit:'
printf '  fpath=(%s $fpath)\n' "$(dirname "$completion")"
