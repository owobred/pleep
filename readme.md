# Pleep
![pleeping cat](media/pleep.webp)

Scaled down version of [owobred/plink](https://github.com/owobred/plink), aiming to allow song recognition from a flat file.

## How to use
### Building the flat file
1. Collect a number of songs into a directory.
2. Navigate to `pleep-build`
3. Run `cargo run -r -- --fft-overlap 2048 -r 20khz --search <songs_dir> --ignore <ignored_file> out.bin` where
    - `--fft-overlap` is the amount of samples each FFT window overlaps with the previous one.
    - `-r` is the sample rate to resample all the audio files to before generating a spectrogram.
    - `--search` is the directory containing the songs.
    - `--ignore` is a file that shouldn't be included in the file for whatever reason.
    - `out.bin` is the output file.
> [!WARNING]
> By default the command will only log warnings, which are unlikely as the program will just panic if it encounters invalid values.
> Consider setting the log level lower by setting the `RUST_LOG` environment variable to a more noisy log level.
> e.g. `RUST_LOG="info" cargo run -r -- <...>` would show info logs 

### Recognizing a song
1. Navigate to `pleep-search`
2. Run `cargo run -r -- <flat_file> <audio_file>` where
    - `flat_file` is the output from the previous command.
    - `audio_file` is the file to recognize.
3. By default, `info` logs will output the top matches of the song (which may not be displayed by default). 
> [!TIP]
> You can have this command output json by passing the `--json` argument before the other arguments.
> For example, `cargo run -r -- --json <flat_file> <audio_file>` will output a json string to stdout.