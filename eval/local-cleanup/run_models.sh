#!/bin/sh
# Run cleanup models over one engine's raw drafts: ./run_models.sh [source] [prompt] [model names...]
# Round 1 was every model with prompt v1 on Parakeet. Models in $MODELS (default ~/vocet-cleanup-models).
set -e
cd "$(dirname "$0")"
M=${MODELS:-$HOME/vocet-cleanup-models}
SRC=${1:-parakeet}; P=${2:-v1}; shift 2 2>/dev/null || true; ONLY="$*"
while read -r name file; do
  [ -f "runs/$SRC/${name}__$P/meta.json" ] && continue
  if [ -n "$ONLY" ]; then case " $ONLY " in *" $name "*) ;; *) continue ;; esac; fi
  python3 -I clean.py --model "$M/$file" --name "$name" --source "$SRC" --prompt "$P"
done <<LIST
qwen3-0.6b Qwen3-0.6B-Q4_K_M.gguf
qwen3-1.7b Qwen3-1.7B-Q4_K_M.gguf
qwen3-4b Qwen3-4B-Q4_K_M.gguf
qwen3-4b-2507 Qwen3-4B-Instruct-2507-Q4_K_M.gguf
qwen3.5-0.8b Qwen3.5-0.8B-Q4_K_M.gguf
qwen3.5-2b Qwen3.5-2B-Q4_K_M.gguf
qwen3.5-4b Qwen3.5-4B-Q4_K_M.gguf
gemma3-1b gemma-3-1b-it-Q4_K_M.gguf
gemma3-4b gemma-3-4b-it-Q4_K_M.gguf
gemma4-e2b gemma-4-E2B-it-Q4_K_M.gguf
gemma4-e4b gemma-4-E4B-it-Q4_K_M.gguf
llama3.2-3b Llama-3.2-3B-Instruct-Q4_K_M.gguf
phi4-mini Phi-4-mini-instruct-Q4_K_M.gguf
smollm3-3b SmolLM3-Q4_K_M.gguf
granite4.2-3b granite-4.2-3b-Q4_K_M.gguf
nemotron3.5-bolt-e4b NVIDIA-Nemotron-3.5-Bolt-E4B-Q4_K_M.gguf
lfm2.5-1.2b LFM2.5-1.2B-Instruct-Q4_K_M.gguf
s1-mini s1-mini-q4_k_m.gguf
bitvoice-0.6b bitvoice-qwen3-0.6b-ft.gguf
minicpm5-2b MiniCPM5-2B-Q4_K_M.gguf
lfm3-1.7b LFM3-1.7B-Q4_K_M.gguf
LIST
