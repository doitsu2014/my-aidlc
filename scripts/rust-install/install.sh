#!/bin/sh
# Experimental Rust installer prototype. Kept separate from the production installer.
set -eu

usage() {
  printf '%s\n' 'Usage: rust-install.sh [--from SOURCE_DIR] [--prefix PREFIX] [--upgrade | --uninstall]' \
    '  SOURCE_DIR must contain target/release/dtech-aidlc[.exe] and assets/' \
    '  --prefix defaults to ~/.local; --upgrade replaces a recognised installation' \
    '  --uninstall removes only recognised files'
}
fail() { printf 'rust-install.sh: %s\n' "$*" >&2; exit 1; }
present() { [ -e "$1" ] || [ -L "$1" ]; }

source_dir=
prefix=${HOME:?HOME must be set}/.local
upgrade=0
uninstall=0
while [ "$#" -gt 0 ]; do
  case $1 in
    --from) [ "$#" -ge 2 ] || fail '--from requires a directory'; source_dir=$2; shift 2 ;;
    --prefix) [ "$#" -ge 2 ] || fail '--prefix requires a directory'; prefix=$2; shift 2 ;;
    --upgrade) upgrade=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done
[ "$upgrade" -eq 0 ] || [ "$uninstall" -eq 0 ] || fail '--upgrade and --uninstall cannot be combined'
case $prefix in /*) ;; *) prefix=$(pwd -P)/$prefix ;; esac
[ "$uninstall" -eq 1 ] || [ -n "$source_dir" ] || source_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)

install_root=$prefix/lib/dtech-aidlc
bin_dir=$prefix/bin
launcher=$bin_dir/dtech-aidlc

# Recognise the Rust install via a private marker. Never infer ownership from a
# generic executable name or remove the legacy Node layout.
is_ours() {
  [ -d "$install_root" ] && [ ! -L "$install_root" ] || return 1
  if [ -f "$install_root/.dtech-aidlc-rust-install" ] &&
    [ -f "$install_root/dtech-aidlc" ] && [ -d "$install_root/assets" ]; then return 0; fi
  # Migration is permitted only for the existing Node package's exact identity and entrypoint.
  [ -f "$install_root/package.json" ] && [ -f "$install_root/core/tools/dtech-aidlc.mjs" ] &&
    grep -q '"name"[[:space:]]*:[[:space:]]*"dtech-aidlc"' "$install_root/package.json"
}
launcher_is_ours() {
  [ -f "$launcher" ] && [ ! -L "$launcher" ] && {
    grep -Fq 'lib/dtech-aidlc/dtech-aidlc' "$launcher" ||
      grep -Fq 'lib/dtech-aidlc/core/tools/dtech-aidlc.mjs' "$launcher"
  }
}
check_recognised() {
  if present "$install_root" && ! is_ours; then fail "$install_root exists but is not a recognised Rust installation; not touching it"; fi
  if present "$launcher" && ! launcher_is_ours; then fail "$launcher exists but is not a recognised Rust launcher; not touching it"; fi
}
if [ "$uninstall" -eq 1 ]; then
  [ ! -d "$prefix" ] && { printf 'nothing to uninstall under %s\n' "$prefix"; exit 0; }
  check_recognised
  if [ -f "$install_root/.dtech-aidlc-rust-install" ]; then rm -f "$launcher"; rm -rf "$install_root"; printf 'Removed Rust installation under %s\n' "$prefix"; exit 0; fi
  # A Node install is not a Rust install: never remove its runtime or launcher.
  printf '%s\n' 'No Rust installation found; existing Node installation left unchanged.'
  exit 0
fi

[ -d "$source_dir" ] || fail "source directory not found: $source_dir"
source_dir=$(CDPATH= cd -- "$source_dir" && pwd -P)
binary=$source_dir/target/release/dtech-aidlc
[ -f "$binary" ] || binary=$source_dir/target/release/dtech-aidlc.exe
[ -f "$binary" ] || fail 'missing target/release/dtech-aidlc executable; build it before installing'
[ -d "$source_dir/assets" ] || fail 'missing assets/ directory'
[ -f "$source_dir/assets/phase-graph.json" ] || fail 'missing assets/phase-graph.json'
if [ "$upgrade" -eq 1 ]; then check_recognised
else
  existing=
  for path in "$install_root" "$launcher"; do present "$path" && existing="$existing${existing:+ }$path"; done
  [ -z "$existing" ] || fail "already exists: $existing; use --upgrade or --uninstall"
fi

mkdir -p "$prefix/lib" "$bin_dir"
stage=$prefix/lib/.dtech-aidlc-rust.new.$$
old=$prefix/lib/.dtech-aidlc-rust.old.$$
tmp_launcher=$bin_dir/.dtech-aidlc-rust.tmp.$$
moved_old=0
moved_new=0
had_launcher=0
moved_launcher=0
present "$launcher" && had_launcher=1
launcher_backup=$bin_dir/.dtech-aidlc-rust.launcher-backup.$$
cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    if [ "$moved_launcher" -eq 1 ]; then rm -f "$launcher"; mv "$launcher_backup" "$launcher";
    elif [ "$had_launcher" -eq 0 ]; then rm -f "$launcher"; fi
    [ "$moved_new" -eq 0 ] || rm -rf "$install_root"
    [ "$moved_old" -eq 0 ] || mv "$old" "$install_root"
  fi
  rm -rf "$stage" "$old" "$tmp_launcher" "$launcher_backup"
  exit "$status"
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$stage/assets"
cp "$binary" "$stage/dtech-aidlc"
cp -R "$source_dir/assets/." "$stage/assets/"
printf '%s\n' 'dtech-aidlc rust installer v1' > "$stage/.dtech-aidlc-rust-install"
(umask 077; cat > "$tmp_launcher" <<'LAUNCHER'
#!/bin/sh
set -eu
self=$0
while [ -L "$self" ]; do
  link=$(readlink "$self")
  case $link in /*) self=$link ;; *) self=$(dirname -- "$self")/$link ;; esac
done
here=$(CDPATH= cd -- "$(dirname -- "$self")" && pwd -P)
entry=$here/../lib/dtech-aidlc/dtech-aidlc
[ -x "$entry" ] || { printf '%s\n' 'dtech-aidlc: Rust installation is incomplete.' >&2; exit 1; }
export DTECH_AIDLC_ASSETS="$here/../lib/dtech-aidlc/assets"
exec "$entry" "$@"
LAUNCHER
)
chmod 755 "$tmp_launcher"
if present "$install_root"; then mv "$install_root" "$old"; moved_old=1; fi
mv "$stage" "$install_root"; moved_new=1
# Back up a recognised existing launcher so failed publication can roll it back.
if [ "$had_launcher" -eq 1 ]; then mv "$launcher" "$launcher_backup"; moved_launcher=1; fi
mv -f "$tmp_launcher" "$launcher"
trap - EXIT HUP INT TERM
rm -rf "$old" "$launcher_backup"
printf 'Installed Rust dtech-aidlc to %s\n' "$launcher"
printf 'Assets installed to %s\n' "$install_root/assets"
