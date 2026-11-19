fn main() {
    let fixtures: &[(&str, &[u8])] = &[
        ("raw-cp1251", b"{\\rtf1\\ansi\\ansicpg1251 \xe9}"),
        ("raw-cp1252", b"{\\rtf1\\ansi\\ansicpg1252 \xe9}"),
        ("font-cp1251", br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset204 Cyrillic;}}\f0\'e9}"),
        ("table", br"{\rtf1\ansi\trowd\cellx1000\cellx2000\intbl Left\cell Right\cell\row}"),
        ("hidden", br"{\rtf1\ansi Visible{\v  hidden} text}"),
        ("footnote", br"{\rtf1\ansi Start{\super\chftn}{\footnote\pard\plain{\super\chftn} Note.} end.\par}"),
        ("unicode", br"{\rtf1\ansi\uc1 Caf\u233\'e9.}"),
        ("paragraphs", br"{\rtf1\ansi Alpha\par Beta}"),
    ];
    for (id, bytes) in fixtures {
        println!("RTF_RESULT {} {:?}", id, deformat::rtf::extract_bytes(bytes).map(|x| x.text));
    }
}