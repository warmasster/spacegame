//! The news: the sections of `LEEME.txt` (what each version brought, the newest on top), cut out
//! and put back into paragraphs. The file is plain text folded by hand at some eighty columns,
//! each version under a line "NOVEDADES V<N> ..."; under the last of them, after a blank line,
//! comes the manual, which is no version's news.

/// One paragraph of the news.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    /// What stands before it: nothing (a paragraph), "-" (an item) or its number ("1.").
    pub mark: String,
    /// The heading in capitals it starts with, if any ("TREN DE ATERRIZAJE"), without its colon.
    pub lead: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct News {
    /// The section's heading ("NOVEDADES V33"); empty if the file has none.
    pub title: String,
    pub blocks: Vec<Block>,
}

/// What a version brought: its section of the file, read, and a line that sums it up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub version: u32,
    pub news: News,
    pub headline: String,
}

/// The version a line heads the section of, if it heads one ("NOVEDADES V35 (...):").
fn version_of(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("NOVEDADES V")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[..digits].parse().ok()
}

/// Whether a line heads a version's section.
fn is_header(line: &str) -> bool {
    version_of(line).is_some()
}

/// The sections of the text, as they come: each version and its part of the text, from its
/// heading to before the next one. The last of several ends at its first blank line: what follows
/// is the manual.
pub fn sections(text: &str) -> Vec<(u32, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut starts = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(version) = version_of(line) {
            starts.push((version, at));
        }
        at += line.len();
    }
    let ends = starts.iter().skip(1).map(|(_, at)| *at).chain([text.len()]);
    let several = starts.len() > 1;
    let mut found: Vec<(u32, &str)> = starts.iter().zip(ends).map(|((version, from), to)| (*version, text[*from..to].trim_end())).collect();
    if several && let Some((_, last)) = found.last_mut() {
        let mut end = 0;
        for line in last.split_inclusive('\n') {
            if line.trim().is_empty() {
                break;
            }
            end += line.len();
        }
        *last = last[..end].trim_end();
    }
    found
}

/// The words of a heading as they are read in a line: those written all in capitals, in small
/// letters ("TREN DE ATERRIZAJE" → "Tren de aterrizaje"); the others, as they are.
fn calm(lead: &str) -> String {
    let words: Vec<String> = lead.split(' ').map(|w| if w.chars().any(char::is_lowercase) { w.to_string() } else { w.to_lowercase() }).collect();
    crate::words::capitalised(&words.join(" "))
}

/// The most headings a headline names.
const HEADLINE_MOST: usize = 5;

/// A line that sums the news up: its first headings ("Pantalla de carga · Andar · Mochila"); with
/// no headings, its first sentence.
pub fn headline(news: &News) -> String {
    let leads: Vec<String> = news.blocks.iter().filter(|b| b.mark.is_empty() && !b.lead.is_empty()).take(HEADLINE_MOST).map(|b| calm(&b.lead)).collect();
    if !leads.is_empty() {
        return leads.join(" · ");
    }
    let Some(first) = news.blocks.iter().find(|b| !b.text.is_empty()) else { return String::new() };
    // (a sentence ends at a stop followed by a space: "F3." in the middle of a number does not)
    let end = first.text.match_indices(". ").next().map_or(first.text.len(), |(at, _)| at);
    first.text[..end].trim_end_matches('.').to_string()
}

/// What each version brought, as the text tells it, in the order it does.
pub fn read_all(text: &str) -> Vec<Section> {
    sections(text)
        .into_iter()
        .map(|(version, part)| {
            let news = read_section(part);
            Section { version, headline: headline(&news), news }
        })
        .collect()
}

/// The first section of the text: from its start up to (not including) the second line that
/// heads a version; all of it if there is no second one.
#[cfg(test)]
pub fn first_section(text: &str) -> &str {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (mut headers, mut at) = (0, 0);
    for line in text.split_inclusive('\n') {
        if is_header(line) {
            headers += 1;
            if headers == 2 {
                break;
            }
        }
        at += line.len();
    }
    text[..at].trim_end()
}

/// A line's mark, if it starts an item ("- ...", "1. ..."), and what follows it.
fn marked(body: &str) -> Option<(&str, &str)> {
    if let Some(rest) = body.strip_prefix("- ") {
        return Some(("-", rest.trim_start()));
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let rest = body[digits..].strip_prefix(". ")?;
    (digits > 0 && digits <= 2).then(|| (&body[..=digits], rest.trim_start()))
}

/// The heading in capitals a paragraph starts with ("TU CUERPO: llevas..."), and the rest: what
/// stands before its first colon, if that is short and written (nearly all) in capitals.
fn lead(body: &str) -> Option<(&str, &str)> {
    let (head, rest) = body.split_once(':')?;
    let letters = head.chars().filter(|c| c.is_alphabetic()).count();
    let capitals = head.chars().filter(|c| c.is_uppercase()).count();
    let first_word_shouts = head.split_whitespace().next().is_some_and(|w| w.chars().all(|c| !c.is_lowercase()));
    let fits = head.chars().count() <= 60 && (rest.is_empty() || rest.starts_with(' '));
    (letters >= 2 && capitals * 10 >= letters * 7 && first_word_shouts && fits).then(|| (head.trim(), rest.trim()))
}

/// The paragraph that is being put together from the lines it was folded into.
struct Open {
    /// Where the lines it goes on in start (an item's are set in under its text).
    indent: usize,
    /// How long its last line is, and whether that line ended a sentence.
    last: usize,
    ended: bool,
    /// It is an item of a list (it has a mark).
    item: bool,
}

/// The news in a text: its first section.
#[cfg(test)]
pub fn read(text: &str) -> News {
    read_section(first_section(text))
}

/// A section as a heading and paragraphs. A line goes on with the paragraph before it unless it
/// starts an item or a heading in capitals, comes back out from under an item, or follows a blank
/// line or — outside the items — a line cut short on purpose (one that ends a sentence and had
/// room for the first word of this one, which starts another).
pub fn read_section(section: &str) -> News {
    let width = section.lines().map(|l| l.trim_end().chars().count()).max().unwrap_or(0);
    let mut news = News::default();
    let mut open: Option<Open> = None;
    for (n, line) in section.lines().enumerate() {
        if n == 0 && is_header(line) {
            news.title = line.split(['(', ':']).next().unwrap_or(line).trim().to_string();
            continue;
        }
        let line = line.trim_end();
        let body = line.trim_start();
        if body.is_empty() {
            open = None;
            continue;
        }
        let indent = line.chars().count() - body.chars().count();
        let last = line.chars().count();
        let ended = body.ends_with(['.', ':', '!', '?', ')']);
        let first_word = body.split_whitespace().next().map_or(0, |w| w.chars().count());
        let item = marked(body);
        let starts_sentence = body.starts_with(|c: char| !c.is_lowercase());
        let cut_short = |o: &Open| !o.item && o.ended && starts_sentence && o.last + 1 + first_word <= width;
        let goes_on = item.is_none() && open.as_ref().is_some_and(|o| indent >= o.indent && !(indent == 0 && lead(body).is_some()) && !cut_short(o));
        if goes_on {
            if let Some(block) = news.blocks.last_mut() {
                if !block.text.is_empty() {
                    block.text.push(' ');
                }
                block.text.push_str(body);
            }
            open = open.map(|o| Open { last, ended, ..o });
            continue;
        }
        let (mark, rest) = item.unwrap_or(("", body));
        let (head, text) = lead(rest).unwrap_or(("", rest));
        news.blocks.push(Block { mark: mark.to_string(), lead: head.to_string(), text: text.to_string() });
        // (an item's next lines stand under its text, after the mark)
        let under = line.chars().count() - rest.chars().count();
        open = Some(Open { indent: if mark.is_empty() { indent } else { under }, last, ended, item: !mark.is_empty() });
    }
    news
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "NOVEDADES V33 (LunaV33_debug.exe / LunaV33_demo.exe):
EL IMAN DE LA GRUA (Cachalote), COMO SE USA:
 1. Suelta el anclaje de la carga: mira su palanca roja (en el suelo, junto a
    la carga) y pulsa E. Sin eso el iman NO se la lleva (va atornillada).
 2. En la consola de la grua (ahora en medio de la bodega, mirando a popa):
    PUENTE y CARRO hasta ponerla encima; IZADO abajo del todo: baja y SE PARA
    SOLA al tocar la carga (lampara TOPE).
 3. Levanta la tapa roja y pon IMAN en AGARRA: lampara SUJETA en verde.
    Si se enciende ANCLADA (roja) es que la carga sigue en su anclaje: paso 1.
 Arriba a la derecha te lo dice con palabras cuando algo se para o no agarra.
TREN DE ATERRIZAJE: las patas son amortiguadores de verdad (muelle de gas y
freno): la nave se hunde al posarse, cabecea y se asienta. Las del Cachalote
son mas grandes. Una toma dura llega al tope.
NADA ATRAVIESA NADA: lo que mueve un mecanismo se para en lo que encuentra:
el izado de la grua en la carga, la rampa en el suelo.
TU CUERPO: llevas el traje puesto. Mira abajo y veras la caja del pecho, los
brazos y, al andar, las piernas.
- ESCALONES: subes sin saltar lo que no pase de 45 cm (un bordillo, el borde
  de la rampa, un palé bajo) y bajas sin quedarte flotando.
- MOCHILA: el estabilizador (Z) ahora mantiene tambien la ALTURA.
INTERFAZ: marco de visor en las esquinas, reloj de mision arriba a la
izquierda, y las cifras (F3) con el mismo cristal que el resto.
Las taquillas de la cabina del Alcotan miran ahora al pasillo.

NOVEDADES V32 (LunaV32_debug.exe / LunaV32_demo.exe):
MODELOS: de cerca ya no se ven cajas.

NOVEDADES V29:
1: soldador-escaner.
";

    #[test]
    fn the_first_section_ends_before_the_second_header() {
        let s = first_section(TEXT);
        assert!(s.starts_with("NOVEDADES V33 ("));
        assert!(s.ends_with("miran ahora al pasillo."));
        assert!(!s.contains("V32"));
    }

    #[test]
    fn a_text_with_one_header_or_none_is_all_of_it() {
        assert_eq!(first_section("NOVEDADES V1:\nuna cosa\n\n"), "NOVEDADES V1:\nuna cosa");
        assert_eq!(first_section("sin cabecera\nNOVEDADES VARIAS: no es una version\n"), "sin cabecera\nNOVEDADES VARIAS: no es una version");
        assert_eq!(first_section(""), "");
    }

    #[test]
    fn what_stands_before_the_first_header_belongs_to_the_first_section() {
        let s = first_section("\u{feff}LUNA\r\nNOVEDADES V3:\r\ntres\r\nNOVEDADES V2:\r\ndos\r\n");
        assert_eq!(s, "LUNA\r\nNOVEDADES V3:\r\ntres");
    }

    #[test]
    fn the_title_is_the_header_without_its_files() {
        assert_eq!(read(TEXT).title, "NOVEDADES V33");
        assert_eq!(read("NOVEDADES V29:\nuna\n").title, "NOVEDADES V29");
        assert_eq!(read("LUNA\nuna\n").title, "");
    }

    #[test]
    fn folded_lines_are_put_back_into_paragraphs() {
        let news = read(TEXT);
        let block = |mark: &str, lead: &str, text: &str| Block { mark: mark.into(), lead: lead.into(), text: text.into() };
        let expected = [
            block("", "EL IMAN DE LA GRUA (Cachalote), COMO SE USA", ""),
            block("1.", "", "Suelta el anclaje de la carga: mira su palanca roja (en el suelo, junto a la carga) y pulsa E. Sin eso el iman NO se la lleva (va atornillada)."),
            block("2.", "", "En la consola de la grua (ahora en medio de la bodega, mirando a popa): PUENTE y CARRO hasta ponerla encima; IZADO abajo del todo: baja y SE PARA SOLA al tocar la carga (lampara TOPE)."),
            block("3.", "", "Levanta la tapa roja y pon IMAN en AGARRA: lampara SUJETA en verde. Si se enciende ANCLADA (roja) es que la carga sigue en su anclaje: paso 1."),
            block("", "", "Arriba a la derecha te lo dice con palabras cuando algo se para o no agarra."),
            block("", "TREN DE ATERRIZAJE", "las patas son amortiguadores de verdad (muelle de gas y freno): la nave se hunde al posarse, cabecea y se asienta. Las del Cachalote son mas grandes. Una toma dura llega al tope."),
            block("", "NADA ATRAVIESA NADA", "lo que mueve un mecanismo se para en lo que encuentra: el izado de la grua en la carga, la rampa en el suelo."),
            block("", "TU CUERPO", "llevas el traje puesto. Mira abajo y veras la caja del pecho, los brazos y, al andar, las piernas."),
            block("-", "ESCALONES", "subes sin saltar lo que no pase de 45 cm (un bordillo, el borde de la rampa, un palé bajo) y bajas sin quedarte flotando."),
            block("-", "MOCHILA", "el estabilizador (Z) ahora mantiene tambien la ALTURA."),
            block("", "INTERFAZ", "marco de visor en las esquinas, reloj de mision arriba a la izquierda, y las cifras (F3) con el mismo cristal que el resto."),
            block("", "", "Las taquillas de la cabina del Alcotan miran ahora al pasillo."),
        ];
        for (got, want) in news.blocks.iter().zip(&expected) {
            assert_eq!(got, want);
        }
        assert_eq!(news.blocks.len(), expected.len());
    }

    #[test]
    fn a_blank_line_ends_a_paragraph() {
        let news = read("NOVEDADES V1:\nuna linea que sigue\nen la siguiente\n\notro parrafo\n");
        let texts: Vec<&str> = news.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, ["una linea que sigue en la siguiente", "otro parrafo"]);
    }

    #[test]
    fn capitals_before_a_colon_are_a_heading_and_other_colons_are_not() {
        assert_eq!(lead("NADA ATRAVIESA NADA: lo que mueve"), Some(("NADA ATRAVIESA NADA", "lo que mueve")));
        assert_eq!(lead("PUENTE y CARRO hasta ponerla encima; IZADO abajo del todo: baja"), None);
        assert_eq!(lead("freno): la nave se hunde"), None);
        assert_eq!(lead("F3: datos en el titulo. F11: pantalla completa."), None);
        assert_eq!(lead("Esc: MENU con pestanas."), None);
        assert_eq!(lead("EL IMAN DE LA GRUA (Cachalote), COMO SE USA:"), Some(("EL IMAN DE LA GRUA (Cachalote), COMO SE USA", "")));
        assert_eq!(lead("sin dos puntos"), None);
        assert_eq!(marked("12. doce"), Some(("12.", "doce")));
        assert_eq!(marked("- una"), Some(("-", "una")));
        assert_eq!(marked("1: soldador"), None);
        assert_eq!(marked("2025. no es un numero de lista"), None);
    }

    #[test]
    fn each_version_has_its_section_and_the_manual_is_nobody_s() {
        let text = format!("{TEXT}\nNAVES:\nDelante del inicio esta el Alcotan.\n\nABRIR: doble clic.\n");
        let parts = sections(&text);
        assert_eq!(parts.iter().map(|(version, _)| *version).collect::<Vec<_>>(), [33, 32, 29]);
        assert!(parts[0].1.starts_with("NOVEDADES V33 (") && parts[0].1.ends_with("miran ahora al pasillo."));
        assert_eq!(parts[1].1, "NOVEDADES V32 (LunaV32_debug.exe / LunaV32_demo.exe):\nMODELOS: de cerca ya no se ven cajas.");
        // (the last one stops at its blank line: the manual follows)
        assert_eq!(parts[2].1, "NOVEDADES V29:\n1: soldador-escaner.");
        // a text with one section keeps all of it, blank lines and all; one with none has none
        assert_eq!(sections("\u{feff}NOVEDADES V1:\r\nuna\r\n\r\notra\r\n"), [(1, "NOVEDADES V1:\r\nuna\r\n\r\notra")]);
        assert_eq!(sections("antes\nNOVEDADES V3:\ntres\nNOVEDADES V2:\ndos\n\nmanual\n"), [(3, "NOVEDADES V3:\ntres"), (2, "NOVEDADES V2:\ndos")]);
        assert!(sections("LUNA\nNOVEDADES VARIAS: no es una version\n").is_empty());
        assert!(sections("").is_empty());
    }

    #[test]
    fn every_section_is_read_as_the_first_one_is() {
        let all = read_all(TEXT);
        assert_eq!(all.iter().map(|s| (s.version, s.news.title.as_str())).collect::<Vec<_>>(), [(33, "NOVEDADES V33"), (32, "NOVEDADES V32"), (29, "NOVEDADES V29")]);
        assert_eq!(all[0].news, read(TEXT));
        assert_eq!(all[1].news.blocks, [Block { mark: String::new(), lead: "MODELOS".into(), text: "de cerca ya no se ven cajas.".into() }]);
        assert!(read_all("sin novedades").is_empty());
    }

    #[test]
    fn a_headline_names_the_first_headings_or_says_the_first_sentence() {
        let all = read_all(TEXT);
        assert_eq!(all[0].headline, "El iman de la grua (Cachalote), como se usa · Tren de aterrizaje · Nada atraviesa nada · Tu cuerpo · Interfaz");
        assert_eq!(all[1].headline, "Modelos");
        assert_eq!(all[2].headline, "1: soldador-escaner");
        assert_eq!(headline(&read("NOVEDADES V30:\nEsc: MENU con pestanas. CONTROLES lista todas las teclas.\nJ: mochila.\n")), "Esc: MENU con pestanas");
        assert_eq!(headline(&read("NOVEDADES V7:\nUna sola frase sin punto\n")), "Una sola frase sin punto");
        assert_eq!(headline(&News::default()), "");
        assert_eq!(calm("ALT, MIRAR SIN GIRAR"), "Alt, mirar sin girar");
        assert_eq!(calm("F12 Y EL Alcotán"), "F12 y el Alcotán");
    }

    #[test]
    fn the_real_file_tells_what_each_version_brought() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LEEME.txt");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let all = read_all(&text);
        assert!(all.len() >= 7, "{}", all.len());
        assert!(all.windows(2).all(|pair| pair[0].version > pair[1].version), "newest first");
        for section in &all {
            assert_eq!(section.news.title, format!("NOVEDADES V{}", section.version));
            assert!(!section.headline.is_empty() && !section.news.blocks.is_empty(), "V{}", section.version);
            assert!(section.news.blocks.iter().all(|b| !b.text.contains("NOVEDADES V")), "V{}", section.version);
        }
        // (the manual under the oldest section is not that version's news)
        assert!(all.last().is_some_and(|s| s.news.blocks.iter().all(|b| b.lead != "ABRIR")));
    }

    #[test]
    fn the_real_file_reads_into_its_newest_version() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LEEME.txt");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let news = read(&text);
        assert!(news.title.starts_with("NOVEDADES V"), "{}", news.title);
        assert!(news.blocks.len() > 5);
        assert!(news.blocks.iter().all(|b| !b.text.contains('\n') && !b.text.contains("NOVEDADES V")));
    }
}
