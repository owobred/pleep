import csv
from pathlib import Path
import os

rows: list[tuple[str, str]] = []

with open("songlist/songlist.csv") as file:
    reader = csv.reader(file)
    for row in reader:
        rows.append(tuple(row))  # type: ignore

to_fetch = []

for hash, filename in rows:
    if not Path(f"segments/{hash}.segment.bin").exists():
        to_fetch.append(filename)

hashes = set(hash for hash, _ in rows)

for file in os.listdir("segments/"):
    if file.removesuffix(".segment.bin") not in hashes:
        print(f"REMOVE: segments/{file}")
        # os.remove(f"segments/{file}")

with open("files_to_fetch.txt", "w") as file:
    file.write("\n".join(to_fetch))