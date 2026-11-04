#!/usr/bin/env bash
# Start a throwaway OpenSSH server in Docker for the quay-ssh integration
# tests and print the environment variables the tests read.
#
#   eval "$(scripts/ssh-test-server.sh start)"
#   cargo test -p quay-ssh --test openssh
#   scripts/ssh-test-server.sh stop
set -euo pipefail

NAME=quay-test-sshd
PORT="${QUAY_TEST_SSH_PORT:-2222}"
DIR="${TMPDIR:-/tmp}/quay-test-ssh"

case "${1:-start}" in
  start)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    mkdir -p "$DIR"
    rm -f "$DIR/id_ed25519" "$DIR/id_ed25519.pub" "$DIR/id_protected" "$DIR/id_protected.pub"
    ssh-keygen -q -t ed25519 -N "" -f "$DIR/id_ed25519"
    ssh-keygen -q -t ed25519 -N "quay-pass" -f "$DIR/id_protected"
    docker run -d --name "$NAME" -p "$PORT:2222" \
      -e PUID=1000 -e PGID=1000 \
      -e PASSWORD_ACCESS=true -e USER_NAME=quay -e USER_PASSWORD=quay-secret \
      -e PUBLIC_KEY="$(cat "$DIR/id_ed25519.pub")
$(cat "$DIR/id_protected.pub")" \
      lscr.io/linuxserver/openssh-server:latest >/dev/null
    # Wait for sshd to accept connections.
    for _ in $(seq 1 60); do
      if (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null; then break; fi
      sleep 1
    done
    sleep 2
    # Jump hosts and tunnels need TCP forwarding, which the image turns off.
    # The tests also fail logins on purpose; without this, OpenSSH 9.8+ starts
    # refusing the whole source address after a few of them.
    docker exec "$NAME" sh -c "sed -i 's/^AllowTcpForwarding no/AllowTcpForwarding yes/' /config/sshd/sshd_config; echo 'PerSourcePenalties no' >> /config/sshd/sshd_config; pkill -HUP sshd" >/dev/null
    sleep 1
    cat <<ENV
export QUAY_TEST_SSH_HOST=127.0.0.1
export QUAY_TEST_SSH_PORT=$PORT
export QUAY_TEST_SSH_USER=quay
export QUAY_TEST_SSH_PASSWORD=quay-secret
export QUAY_TEST_SSH_KEY=$DIR/id_ed25519
export QUAY_TEST_SSH_KEY_PROTECTED=$DIR/id_protected
export QUAY_TEST_SSH_KEY_PASSPHRASE=quay-pass
ENV
    ;;
  stop)
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    ;;
  *)
    echo "usage: $0 start|stop" >&2; exit 2;;
esac
