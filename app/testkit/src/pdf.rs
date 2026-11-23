//! PDFs a test makes: one page of text, and the same page with a filled form field.

/// A one-page PDF that says `text`, which must hold no `(`, `)` or `\`.
#[must_use]
pub fn minimal_pdf(text: &str) -> Vec<u8> {
    pdf(text, "", "", &[])
}

/// [`minimal_pdf`] with one text field filled with `value`, as a filled W-9 is: the value is in
/// the form, not on the page.
#[must_use]
pub fn filled_form_pdf(text: &str, value: &str) -> Vec<u8> {
    let field = format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (ssn) /V ({value}) /Rect [20 20 180 40] \
         /P 3 0 R >>"
    );
    pdf(
        text,
        "/AcroForm << /Fields [6 0 R] >> ",
        "/Annots [6 0 R] ",
        &[field],
    )
}

fn pdf(text: &str, catalog: &str, page: &str, more: &[String]) -> Vec<u8> {
    let content = format!("BT /F1 24 Tf 20 100 Td ({text}) Tj ET");
    let stream = format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    );
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog}>>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
             /MediaBox [0 0 200 200] /Contents 5 0 R {page}>>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        stream,
    ];
    objects.extend_from_slice(more);
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}
