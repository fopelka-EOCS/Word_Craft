//! Hand-written synthetic Romanian test data. No words, rules or suggestion tables were copied
//! from a system dictionary. This is a small fixture, not a redistributable Romanian dictionary.

pub const AFF: &str = "SET UTF-8
FLAG UTF-8
FORBIDDENWORD F
TRY aăâîeiourtnscșțlmpdbgfvzhj
KEY qwertyuiop|asdfghjkl|zxcvbnm
MAP 3
MAP aăâî
MAP sşș
MAP tţț
REP 1
REP ce che
SFX A Y 1 # synthetic definite article -ul
SFX A 0 ul .
PFX N Y 1 # synthetic ne-
PFX N 0 ne .
";

pub const DIC: &str = "32
casă
carte
copil/A
școală
țară
și
în
într-o
până
când
sunt
suntem
sunteți
sânt
este
mâine
român/A
România
București
frumos/N
mult
foarte
bine
de
la
pe
să
dă
spune
are
nu
zqinterzis/F
";

pub fn install() {
    if let Err(e) = super::install_dictionary(AFF, DIC) {
        panic!("test dictionary: {e}");
    }
}
