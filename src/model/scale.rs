use nih_plug::prelude::Enum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Enum)]
pub enum RootNote {
    #[default]
    #[id = "c"]
    C,
    #[id = "cs"]
    #[name = "C#"]
    CSharp,
    #[id = "d"]
    D,
    #[id = "ds"]
    #[name = "D#"]
    DSharp,
    #[id = "e"]
    E,
    #[id = "f"]
    F,
    #[id = "fs"]
    #[name = "F#"]
    FSharp,
    #[id = "g"]
    G,
    #[id = "gs"]
    #[name = "G#"]
    GSharp,
    #[id = "a"]
    A,
    #[id = "as"]
    #[name = "A#"]
    ASharp,
    #[id = "b"]
    B,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Enum)]
pub enum ScaleMode {
    #[default]
    #[id = "minp"]
    #[name = "Minor Pentatonic"]
    MinorPentatonic,
    #[id = "majp"]
    #[name = "Major Pentatonic"]
    MajorPentatonic,
    #[id = "maj"]
    Major,
    #[id = "min"]
    Minor,
    #[id = "chrom"]
    Chromatic,
}
