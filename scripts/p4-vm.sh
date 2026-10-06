#!/usr/bin/env bash
# Second Locus server VM for P4 (and later) OCC. Does not use or kill
# ~/vms/locus-server (notes-app: SSH 22221, Eyre 8080).
set -euo pipefail
export LOCUS_VM_NAME="${LOCUS_VM_NAME:-locus-p4}"
export LOCUS_SSH_PORT="${LOCUS_SSH_PORT:-22222}"
export LOCUS_HTTP_PORT="${LOCUS_HTTP_PORT:-8081}"
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/server-vm.sh" "$@"
