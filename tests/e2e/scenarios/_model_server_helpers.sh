# The stand-in model server (fixtures/model-server.pl, U9) for the scenarios that
# go through a download: deterministic, and no internet. It serves the real model,
# since the app checks the catalogue's SHA-256, so the file comes from this Mac
# over the VM's local network. Sourced by run.sh; defines no scenario.
#
# E2E_MODEL_FILE names the file. By default it's the one a run of the app on this
# Mac downloaded. Without one these scenarios fail and say so: they never fetch it.

MODEL_FILE_NAME=parakeet-unified-en-0.6b-Q4_K_M.gguf
MODEL_FILE_BYTES=477274496
MODEL_FILE_HOST=${E2E_MODEL_FILE:-$HOME/Library/Application Support/com.dictationapp.desktop/models/$MODEL_FILE_NAME}
VM_MODEL_SERVER="$VM_FIXTURES/model-server.pl"
VM_MODEL_SERVER_DIR="/Users/$VM_USER/e2e-model-server"
VM_MODEL_FILE="/Users/$VM_USER/e2e-model/$MODEL_FILE_NAME"
MODEL_SERVER_PORT=18732
MODEL_SERVER_URL="http://127.0.0.1:$MODEL_SERVER_PORT"
# Far enough for a part on disk and nowhere near the end.
MODEL_HOLD_AT=30000000

# The model is in the VM, copied once per reset.
model_file_in_vm() {
  local have
  have=$(vm "stat -f %z $(q "$VM_MODEL_FILE") 2>/dev/null || echo 0")
  [ "$have" = "$MODEL_FILE_BYTES" ] && return 0
  if [ "$(stat -f %z "$MODEL_FILE_HOST" 2>/dev/null)" != "$MODEL_FILE_BYTES" ]; then
    log "  no model at $MODEL_FILE_HOST: run the app's download once on this Mac, or set E2E_MODEL_FILE"
    return 1
  fi
  log "  copying the model into the VM"
  vm "mkdir -p $(q "$(dirname "$VM_MODEL_FILE")")"
  VM_CMD_TIMEOUT=600 vm "cat >$(q "$VM_MODEL_FILE")" <"$MODEL_FILE_HOST"
}

# Starts the server, holding every download at MODEL_HOLD_AT.
model_server_start() {
  local i
  model_file_in_vm || return 1
  model_server_stop || true
  vm "rm -rf $(q "$VM_MODEL_SERVER_DIR") && mkdir -p $(q "$VM_MODEL_SERVER_DIR") && echo $MODEL_HOLD_AT >$(q "$VM_MODEL_SERVER_DIR/hold-at")"
  vm "nohup perl $(q "$VM_MODEL_SERVER" "$VM_MODEL_SERVER_DIR" "$VM_MODEL_FILE" "$MODEL_SERVER_PORT") >$(q "$VM_MODEL_SERVER_DIR/server.log") 2>&1 </dev/null &"
  for i in $(seq 1 20); do
    if [ "$(vm "curl -s -m 2 $MODEL_SERVER_URL/health" 2>/dev/null)" = ok ]; then
      return 0
    fi
    sleep 0.5
  done
  log "  the model server didn't answer: $(vm "cat $(q "$VM_MODEL_SERVER_DIR/server.log")" 2>&1)"
  return 1
}

model_server_stop() {
  vm "pkill -f '[m]odel-server.pl'; i=0; while pgrep -f '[m]odel-server.pl' >/dev/null && [ \$i -lt 20 ]; do sleep 0.25; i=\$((i + 1)); done; ! pgrep -f '[m]odel-server.pl' >/dev/null"
}

# Lets the download finish.
model_server_release() {
  vm "rm -f $(q "$VM_MODEL_SERVER_DIR/hold-at")"
}

# The server was asked to continue from a byte above 0.
model_server_saw_resume() {
  vm "grep -Eq 'request from=[1-9]' $(q "$VM_MODEL_SERVER_DIR/events")"
}
