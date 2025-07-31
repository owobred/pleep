import csv
from pathlib import PurePath

with open("temp_songlist.txt") as hashes:
    lines = hashes.readlines()

lines = [line.split("  ", 1) for line in lines]
lines = [(hash, PurePath(filename[:-1])) for hash, filename in lines]
lines.sort(key=lambda pair: (len(pair[1].parents), pair[1]))

with open("new_songlist.csv", "w") as output:
    writer = csv.writer(output)
    writer.writerows(lines)
