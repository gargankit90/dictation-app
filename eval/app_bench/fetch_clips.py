# /// script
# requires-python = ">=3.11"
# dependencies = ["pyarrow>=15", "soundfile>=0.12", "numpy"]
# ///
"""Download the public Dictation Dataset (151 clips, VoiceArena/Dictation_Dataset on Hugging Face, CC BY-NC 4.0)
and lay it out for the harness. This project is not affiliated with the dataset's authors.

Writes <out>/<id>.flac for every clip and <out>/manifest.csv (the harness's manifest columns plus
spoken_text, intended_text and domain for the scorer). The audio is CC BY-NC 4.0 and must
never be committed: clips/ is git-ignored.

    uv run fetch_clips.py clips
"""
import argparse, csv, hashlib, io, pathlib, urllib.request

import pyarrow.parquet as pq
import soundfile as sf

REVISION = "ab671bb28d33defac2955fda2a588e75599a368b"  # dataset commit used for this run
URL = (f"https://huggingface.co/datasets/VoiceArena/Dictation_Dataset/resolve/{REVISION}"
       "/data/test-00000-of-00001.parquet")
SHA256 = "0b13f09bc5f5a326df0a7b002159b627ddcba37af0f19bbc413934c3a0bbfe80"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=pathlib.Path)
    ap.add_argument("--parquet", type=pathlib.Path, help="use an already downloaded parquet")
    a = ap.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    data = a.parquet.read_bytes() if a.parquet else urllib.request.urlopen(URL).read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != SHA256:
        raise SystemExit(f"parquet sha256 {digest} != expected {SHA256}")
    t = pq.read_table(io.BytesIO(data)).to_pylist()
    # file/reference_text/type/task_type/speaker_id/audio_duration_s are the harness's manifest columns
    # (dictation_common._loop); spoken_text/intended_text/domain are what the scorer needs.
    cols = ["file", "clip_id", "reference_text", "type", "task_type", "speaker_id", "audio_duration_s",
            "spoken_text", "intended_text", "domain", "audio_length_s"]
    with open(a.out / "manifest.csv", "w", newline="") as f:
        w = csv.DictWriter(f, cols)
        w.writeheader()
        for r in t:
            audio = r["audio"]["bytes"]
            (a.out / f"{r['id']}.flac").write_bytes(audio)
            info = sf.info(io.BytesIO(audio))
            assert info.channels == 1, r["id"]
            w.writerow({"file": f"{r['id']}.flac", "clip_id": r["id"], "reference_text": r["intended_text"],
                        "type": "speech", "task_type": r["domain"], "speaker_id": r["speaker_id"],
                        "audio_duration_s": f"{info.duration:.2f}", "spoken_text": r["spoken_text"],
                        "intended_text": r["intended_text"], "domain": r["domain"],
                        "audio_length_s": r["audio_length_s"]})
    print(f"{len(t)} clips -> {a.out}")


if __name__ == "__main__":
    main()
