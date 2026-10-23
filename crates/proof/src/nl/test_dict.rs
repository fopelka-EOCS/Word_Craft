//! Small synthetic Dutch Hunspell dictionary for unit and integration tests.
//!
//! Covers the features [`crate::nl`] relies on from OpenTaal (`nl_NL` / `nl_BE`):
//!
//! - `TRY`, `KEY`, `MAP` (`eéèëê`, `aáàäâ`, `iíìïî`, `oóòöô`, `uúùüû`, `cç`, `nñ`) and `REP`
//!   (`d`/`t`/`dt`, `ei`/`ij`, `au`/`ou`, `os$`/`o's`, `as$`/`a's`, `us$`/`u's`, missing diaeresis)
//! - `FORBIDDENWORD` (`gebeurdt`, `worddt`, `vinddt`) and `NOSUGGEST`
//! - `PFX` (`on-`, `her-`) and `SFX` for plurals (`-en`, `-ën`, `-s`, `-'s`), diminutives
//!   (`-je`, `-tje`, `-'tje`), adjective inflections (`-e`, `-er`, `-ere`, `-st`, `-ste`),
//!   verb conjugations (`-t`, `-de`, `-den`, `-te`, `-ten`, `-en`) and proper-name possessives
//! - Proper nouns (`Nederland`, `Amsterdam`, `België`, `Alex`, `Anna`) and hyphenated parts

use std::sync::Arc;

use crate::affix::AffixDict;

pub const AFF: &str = "\
SET UTF-8
TRY esiaonrtldkumgcbpvwjfhzyxéèëêïöü'
KEY qwertyuiop|asdfghjkl|zxcvbnm
FORBIDDENWORD !
NOSUGGEST ~

MAP 7
MAP eéèëê
MAP aáàäâ
MAP iíìïî
MAP oóòöô
MAP uúùüû
MAP nñ
MAP cç

REP 26
REP dt$ d
REP dt$ t
REP d$ dt
REP t$ d
REP d$ t
REP ei ij
REP ij ei
REP au ou
REP ou au
REP os$ o's
REP as$ a's
REP us$ u's
REP ys$ y's
REP tje 'tje
REP 'tje tje
REP eë e
REP ee ë
REP eeë eë
REP ieë ië
REP g ch
REP ch g
REP s z
REP z s
REP f v
REP v f
REP ^nietwaar$ niet_waar

PFX O Y 1
PFX O 0 on .

PFX H Y 1
PFX H 0 her .

SFX Z Y 4
SFX Z 0 en [^eo]
SFX Z 0 ën [eo]
SFX Z d den d
SFX Z t ten t

SFX S Y 1
SFX S 0 s .

SFX C Y 1
SFX C 0 's .

SFX J Y 2
SFX J 0 je .
SFX J 0 jes .

SFX T Y 2
SFX T 0 tje .
SFX T 0 tjes .

SFX Y Y 2
SFX Y 0 'tje .
SFX Y 0 'tjes .

SFX A Y 5
SFX A 0 e [^e]
SFX A 0 er [^r]
SFX A 0 ere [^r]
SFX A 0 st [^s]
SFX A 0 ste [^s]

SFX V Y 4
SFX V 0 t .
SFX V 0 de .
SFX V 0 den .
SFX V 0 en .

SFX W Y 4
SFX W 0 t .
SFX W 0 te .
SFX W 0 ten .
SFX W 0 en .

SFX P N 3
SFX P 0 ' [hsxz]
SFX P 0 's [aiouy]
SFX P 0 s [bcdefgjklmnprtvw]
";

pub const DIC: &str = "\
260
de
het
een
en
of
maar
want
omdat
dat
wat
wie
waar
waarom
wanneer
hoe
welk/A
deze
dit
die
ik
jij
je
u
hij
zij
ze
wij
we
jullie
hun
hen
mij
me
jou
hem
haar
ons
mijn
jouw
uw
zijn
onze
zich
zelf
elkaar
in
op
aan
uit
van
voor
naar
met
bij
tot
over
onder
tussen
door
tegen
zonder
binnen
buiten
achter
naast
sinds
tijdens
volgens
wegens
rond
vanaf
vanuit
niet
geen
wel
ook
nog
al
reeds
altijd
nooit
vaak
soms
misschien
graag
heel/A
hele
zeer
erg/A
echt/A
bijna
ongeveer
alleen
samen
weer
terug
hier
daar
ergens
nergens
overal
nu
dan
toen
straks
vandaag
gisteren
morgen
morgens
avond/Z
avonds
middag/Z
middags
ochtend/Z
ochtends
winter/S
winters
zomer/S
zomers
eerder/A
later/A
goed/A
beter/A
best/A
slecht/A
veel
meer
meest/A
weinig/A
minder
minst/A
ander/A
anderen
elk/A
ieder/A
alle
alles
iedereen
niemand
iets
niets
enkele
verschillend/A
eigen
zelfde
half
halve
eerste
tweede
derde
laatste
volgend/A
vorig/A
ja
nee
nou
zo
toch
echter
daarom
daarna
daardoor
daarbij
namelijk
bijvoorbeeld
inderdaad
natuurlijk/A
uiteraard
helaas
gelukkig/AO
eigenlijk/A
waarschijnlijk/AO
mogelijk/AO
nodig/AO
duidelijk/AO
makkelijk/AO
moeilijk/A
eerlijk/AO
heerlijk/A
vriendelijk/AO
hartelijk/A
persoonlijk/A
redelijk/AO
onmiddellijk/A
uiteindelijk/A
mooi/A
gezellig/AO
belangrijk/AO
groot
grote
groter
grootst/A
klein/A
nieuw/A
oud/A
jong/A
lang/A
kort/A
snel
snelle
sneller
snelst/A
hoog
hoge
hoger
hoogst/A
laag
lage
lager
laagst/A
warm/A
koud/A
blij/A
vrij/A
rijk/A
arm/A
sterk/A
zwak
zwakke
Nederland/P
Nederlands/A
Nederlander/S
Amsterdam/P
Rotterdam/P
Utrecht/P
België
Vlaanderen
Europa
Alex/P
Max/P
Anna/P
Jan/P
Sanne/P
WordCraft
's-Gravenhage
's-Hertogenbosch
café/S
één
tweeën
drieën
reëel
reële
idee/ZT
industrieel
industriële
financiële
officiële
geïnteresseerd/A
coördinatie
poëzie
ruïne/S
geüpdatet
soufflé/S
enquête/S
gênant/A
bèta
crème/S
scène/S
maïs
zee/Z
knie/Z
bacteriën
melodieën
koloniën
tafel/ST
stoel/Z
huis/J
huizen
kamer/S
deur/Z
raam
ramen
straat
straten
stad
steden
dorp/ZJ
land/ZJ
water
lucht
zon
maan
dag/Z
dagen
nacht/Z
jaar
jaren
maand/Z
week
weken
uur
uren
minuut
minuten
mens/Z
man
mannen
vrouw/Z
kind
kinderen
vriend/Z
vriendin
vriendinnen
school
scholen
werk/WJH
kantoor
kantoren
bedrijf
bedrijven
computer/S
telefoon/S
e-mail/S
mail/S
brief
brieven
boek/ZJ
krant/Z
woord/ZJ
woordenboek/Z
tekst/Z
taal
talen
spelling
controle/S
spellingcontrole
grammatica
fout/ZA
voorbeeld/Z
vraag
vragen
antwoord/ZV
probleem
problemen
oplossing/Z
beslissing/Z
vergadering/Z
samenwerking/Z
verband/Z
afspraak
afspraken
koffie
thee
brood/J
kaas
melk
appel/S
peer
peren
geld
prijs
prijzen
kleur/Z
naam
namen
plaats/Z
weg/Z
plein/ZJ
station/S
luchthaven/S
reis/ZV
rijst
vakantie/S
feest/ZJ
muziek
film/S
kunst/Z
geschiedenis
toekomst
wereld
leven/S
hart/J
hoofd/ZJ
hand/Z
oog
ogen
oor
oren
mond/Z
voet/Z
been
benen
fiets/ZWJ
trein/Z
auto/C
foto/C
baby/CY
paraplu/C
menu/C
collega/C
oma/C
opa/C
papa/C
mama/C
tutu/C
radio/C
video/C
pagina/C
tijd/Z
wijn/Z
ijs/J
verkeer
woon/V
speel/V
gebeur/V
bedoel/V
word
wordt
worden
werd
werden
geworden
vind
vindt
vinden
vond
vonden
gevonden
houd
houdt
houden
hield
gehouden
heet
heten
noem/V
zeg
zegt
zeggen
zei
zeiden
gezegd
doe
doet
doen
deed
gedaan
ga
gaat
gaan
ging
gegaan
kom
komt
komen
kwam
gekomen
zie
ziet
zien
zag
gezien
heb
hebt
heeft
hebben
had
hadden
gehad
ben
bent
is
zijn
was
waren
geweest
kan
kunt
kunnen
kon
konden
zal
zult
zullen
zou
zouden
wil
wilt
willen
wilde
wilden
mag
mogen
mocht
moet
moeten
moest
moesten
schrijf
schrijft
schrijven
schreef
geschreven
lees
leest
lezen
las
gelezen
loop
loopt
lopen
liep
gelopen
kijk
kijkt
kijken
keek
gekeken
blijf
blijft
blijven
bleef
gebleven
ex
noord
zuid
oost
west
gebeurdt/!
worddt/!
vinddt/!
scheldwoord/~
";

/// Install the test dictionary as the process's Dutch dictionary (built once).
pub fn install() {
    static BUILT: std::sync::OnceLock<Arc<AffixDict>> = std::sync::OnceLock::new();
    let d = BUILT.get_or_init(|| Arc::new(super::parse_hunspell(AFF, DIC).unwrap_or_default())).clone();
    super::set_installed(d);
}
