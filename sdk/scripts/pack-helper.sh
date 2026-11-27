#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
package="$root/service-darwin-arm64"
destination="$package/bin/hex-service"
binary=""

while [ $# -gt 0 ]; do
  case "$1" in
    --binary)
      [ $# -ge 2 ] || { echo "--binary needs a path" >&2; exit 2; }
      binary=$2
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 --binary /path/to/hex" >&2
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 --binary /path/to/hex" >&2
      exit 2
      ;;
  esac
done

[ -n "$binary" ] || { echo "Usage: $0 --binary /path/to/hex" >&2; exit 2; }
[ -f "$binary" ] || { echo "Not a file: $binary" >&2; exit 1; }

mkdir -p "$(dirname "$destination")" "$root/dist"
cp "$binary" "$destination"
chmod 755 "$destination"
npm pack "$package" --pack-destination "$root/dist"
