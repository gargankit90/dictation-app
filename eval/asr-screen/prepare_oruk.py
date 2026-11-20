"""Repackage the Oruk Transformers 5 checkpoint for transcribe.cpp's Qwen converter."""

import argparse
import json
from pathlib import Path
import shutil
import struct


def tensor_name(name):
    prefixes = (
        ("model.audio_tower.", "thinker.audio_tower."),
        ("model.language_model.", "thinker.model."),
        ("model.multi_modal_projector.linear_1.", "thinker.audio_tower.proj1."),
        ("model.multi_modal_projector.linear_2.", "thinker.audio_tower.proj2."),
    )
    for old, new in prefixes:
        if name.startswith(old):
            return new + name[len(old):]
    if name == "__metadata__":
        return name
    raise ValueError(f"Unexpected tensor name {name}")


def prepare(source, destination):
    destination.mkdir(parents=True, exist_ok=False)

    def read(name):
        return json.loads((source / name).read_text(encoding="utf-8"))

    def write(name, value):
        (destination / name).write_text(json.dumps(value), encoding="utf-8")

    config = read("config.json")
    tokenizer = read("tokenizer.json")
    special = {entry["content"]: entry["id"] for entry in tokenizer["added_tokens"]}
    if config["audio_token_id"] != special["<|audio_pad|>"]:
        raise ValueError("Audio token ID disagrees with the tokenizer")
    config["audio_start_token_id"] = special["<|audio_start|>"]
    config["audio_end_token_id"] = special["<|audio_end|>"]
    audio = config["audio_config"]
    audio["max_source_positions"] = audio["max_position_embeddings"]
    text = config["text_config"]
    text["rope_theta"] = text["rope_parameters"]["rope_theta"]
    write("config.json", {"thinker_config": config, "support_languages": ["English"]})
    write("preprocessor_config.json", read("processor_config.json")["feature_extractor"])
    write("vocab.json", tokenizer["model"]["vocab"])
    merges = [" ".join(pair) if isinstance(pair, list) else pair for pair in tokenizer["model"]["merges"]]
    (destination / "merges.txt").write_text("#version: 0.2\n" + "\n".join(merges) + "\n", encoding="utf-8")
    tokenizer_config = read("tokenizer_config.json")
    tokenizer_config["added_tokens_decoder"] = {str(entry["id"]): entry for entry in tokenizer["added_tokens"]}
    write("tokenizer_config.json", tokenizer_config)
    write("chat_template.json", {"chat_template": (source / "chat_template.jinja").read_text(encoding="utf-8")})

    # Only the safetensors header changes. Offsets are relative to the tensor payload.
    with (source / "model.safetensors").open("rb") as original:
        header_size = struct.unpack("<Q", original.read(8))[0]
        header = json.loads(original.read(header_size))
        renamed = {tensor_name(key): value for key, value in header.items()}
        if len(renamed) != len(header):
            raise ValueError("Tensor names collide after conversion")
        packed = json.dumps(renamed, separators=(",", ":")).encode("utf-8")
        packed += b" " * (-len(packed) % 8)
        with (destination / "model.safetensors").open("xb") as output:
            output.write(struct.pack("<Q", len(packed)))
            output.write(packed)
            shutil.copyfileobj(original, output, length=1024 * 1024)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path, help="a new directory on a volume with 4.1 GB available")
    args = parser.parse_args()
    prepare(args.source, args.destination)
