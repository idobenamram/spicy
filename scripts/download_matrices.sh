#!/usr/bin/env bash
set -euo pipefail

# Download MatrixMarket files into assets/matrices.
#
# Expected usage:
#   bash scripts/download_matrices.sh

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/assets/matrices"

mkdir -p "$DEST"

echo "Matrix directory:"
echo "  $DEST"

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but was not found in PATH." >&2
  exit 1
fi

use_progress=0
if [[ -t 1 ]] && [[ -n "${TERM:-}" ]] && command -v tput >/dev/null 2>&1; then
  use_progress=1
fi

tmp_dir="$(mktemp -d "/tmp/spicy-matrices.XXXXXX")"
status_dir="$tmp_dir/status"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
mkdir -p "$status_dir"

log_line() {
  if [[ "$use_progress" -eq 0 ]]; then
    printf '%s\n' "$1"
  fi
}

set_status() {
  printf '%s\n' "$2" > "$status_dir/$1.status"
}

get_status() {
  if [[ -f "$status_dir/$1.status" ]]; then
    cat "$status_dir/$1.status"
  else
    echo "pending"
  fi
}

get_content_length() {
  local url="$1"
  local length=""
  while IFS= read -r line; do
    line="${line%$'\r'}"
    case "$line" in
      [Cc]ontent-[Ll]ength:\ *)
        length="${line#*: }"
        ;;
    esac
  done < <(curl -sI -L --connect-timeout 5 --max-time 10 "$url" || true)

  if [[ "$length" =~ ^[0-9]+$ ]]; then
    printf '%s' "$length"
  fi
}

format_bytes() {
  local bytes="$1"
  local units=(B KB MB GB TB)
  local unit_index=0
  local value="$bytes"

  while (( value >= 1024 && unit_index < ${#units[@]} - 1 )); do
    value=$((value / 1024))
    unit_index=$((unit_index + 1))
  done

  printf '%s%s' "$value" "${units[$unit_index]}"
}

progress_bar() {
  local percent="$1"
  local width="$2"
  local filled=$((percent * width / 100))
  local empty=$((width - filled))

  printf '%*s' "$filled" '' | tr ' ' '#'
  printf '%*s' "$empty" '' | tr ' ' '-'
}

render_progress() {
  local cols
  local bar_width

  cols="$(tput cols 2>/dev/null || echo 80)"
  bar_width=$((cols - 44))
  if (( bar_width < 10 )); then
    bar_width=10
  elif (( bar_width > 40 )); then
    bar_width=40
  fi

  for i in "${!names[@]}"; do
    local name="${names[$i]}"
    local tarball="${tarballs[$i]}"
    local total="${totals[$i]}"
    local status
    local downloaded=0

    status="$(get_status "$name")"
    if [[ -f "$tarball" ]]; then
      if [[ "$(uname -s)" == "Darwin" ]]; then
        downloaded="$(stat -f%z "$tarball" 2>/dev/null || echo 0)"
      else
        downloaded="$(stat -c%s "$tarball" 2>/dev/null || echo 0)"
      fi
    fi

    if [[ "$total" =~ ^[0-9]+$ && "$total" -gt 0 ]]; then
      local percent=$((downloaded * 100 / total))
      if (( percent > 100 )); then
        percent=100
      fi
      local bar
      bar="$(progress_bar "$percent" "$bar_width")"
      printf '%-14s [%s] %3s%% %-11s\n' "$name" "$bar" "$percent" "$status"
    else
      printf '%-14s %8s %-11s\n' "$name" "$(format_bytes "$downloaded")" "$status"
    fi
  done
}

progress_loop() {
  local count="${#names[@]}"
  if (( count == 0 )); then
    return
  fi

  render_progress
  while :; do
    local running=0
    for pid in "${pids[@]}"; do
      if kill -0 "$pid" 2>/dev/null; then
        running=1
        break
      fi
    done
    if (( running == 0 )); then
      break
    fi
    sleep 0.3
    tput cuu "$count"
    render_progress
  done
}

download_matrix() {
  local name="$1"
  local url="$2"
  local work_dir="$tmp_dir/$name"
  local tarball="$work_dir/$name.tar.gz"

  mkdir -p "$work_dir"

  trap 'set_status "$name" "failed"' ERR

  set_status "$name" "downloading"
  log_line "Downloading ${name}..."
  curl -fL --retry 3 --retry-delay 2 --silent --show-error -o "$tarball" "$url"

  set_status "$name" "extracting"
  log_line "Extracting ${name}..."
  tar -xzf "$tarball" -C "$work_dir"

  local mtx_path=""
  while IFS= read -r -d '' candidate; do
    mtx_path="$candidate"
    break
  done < <(find "$work_dir" -type f -name "${name}.mtx" -print0)
  if [[ -z "$mtx_path" ]]; then
    local mtx_candidates=()
    while IFS= read -r -d '' candidate; do
      mtx_candidates+=("$candidate")
    done < <(find "$work_dir" -type f \( -name "*.mtx" -o -name "*.MTX" \) -print0)

    if [[ "${#mtx_candidates[@]}" -eq 1 ]]; then
      mtx_path="${mtx_candidates[0]}"
    elif [[ "${#mtx_candidates[@]}" -gt 1 ]]; then
      echo "Found multiple .mtx files for $name:" >&2
      printf '  - %s\n' "${mtx_candidates[@]}" >&2
      exit 1
    else
      echo "No .mtx file found for $name." >&2
      exit 1
    fi
  fi

  local dest_path="$DEST/${name}.mtx"
  if [[ "$(basename "$mtx_path")" != "${name}.mtx" ]]; then
    dest_path="$DEST/$(basename "$mtx_path")"
  fi

  cp "$mtx_path" "$dest_path"
  set_status "$name" "done"
  log_line "Saved ${name} -> ${dest_path}"
}

pids=()
names=()
tarballs=()
totals=()
start_download() {
  local name="$1"
  local url="$2"
  local tarball="$tmp_dir/$name/$name.tar.gz"
  local total=""

  set_status "$name" "queued"
  if [[ "$use_progress" -eq 1 ]]; then
    total="$(get_content_length "$url")"
  fi
  download_matrix "$name" "$url" &
  pids+=("$!")
  names+=("$name")
  tarballs+=("$tarball")
  totals+=("$total")
}

# https://sparse.tamu.edu/Freescale/FullChip
start_download "FullChip" "https://suitesparse-collection-website.herokuapp.com/MM/Freescale/FullChip.tar.gz"

# https://sparse.tamu.edu/Freescale/circuit5M
start_download "circuit5M" "https://suitesparse-collection-website.herokuapp.com/MM/Freescale/circuit5M.tar.gz"

# https://www.cise.ufl.edu/research/sparse/matrices/Sandia/ASIC_100k.html
start_download "ASIC_100k" "https://www.cise.ufl.edu/research/sparse/MM/Sandia/ASIC_100k.tar.gz"

# https://www.cise.ufl.edu/research/sparse/matrices/Sandia/adder_dcop_01.html
start_download "adder_dcop_01" "https://www.cise.ufl.edu/research/sparse/MM/Sandia/adder_dcop_01.tar.gz"

if [[ "$use_progress" -eq 1 ]]; then
  progress_loop
fi

fail=0
for i in "${!pids[@]}"; do
  if ! wait "${pids[$i]}"; then
    echo "Download failed: ${names[$i]}" >&2
    set_status "${names[$i]}" "failed"
    fail=1
  fi
done

if [[ "$use_progress" -eq 1 ]]; then
  render_progress
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
