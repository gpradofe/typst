// Enterprise financial-report template — reproduces the memory cost drivers
// from a real-world workload that hit ~17 GB RAM at ~253K rows.
//
// Captures (in approximate proportion to the original):
//   - 4-file JSON load (disclaimers, metadata, advisor_und, ticket_fee_detail)
//   - 3 levels of nesting (groups[].sub_groups[].sub_groups[].rows[])
//   - 10 columns with per-column format-fn dispatch
//   - Per-cell `if "key" in row { ... } else { none }` conditional
//   - layout(size => ...) measure+truncate in every page header
//   - Logo placeholder image in every page header
//   - 3-paragraph footer disclaimer on every page
//   - Continuation header with breadcrumb (state.at(here()))
//   - 3 cell stroke styles (thin, thick, dashed-row-separator)
//   - 5-level heading hierarchy for PDF bookmarks (levels 3-5 hidden size:0pt)
//   - 3 levels of group totals walking an agg-fields tuple
//   - Custom number formatting (commas, parens-for-negatives, decimals per type)
//   - Conditional page breaks + per-class margin lookup
//   - Optional end-disclaimer page
//
// Usage:
//   typst compile --root benchmarks enterprise_report.typ output.pdf \
//     --input dataroot=data/enterprise_100k

#let dataroot = sys.inputs.at("dataroot", default: "data/enterprise_10k")

#let disclaimers = json("/" + dataroot + "/disclaimers.json")
#let metadata = json("/" + dataroot + "/metadata.json")
#let advisor = json("/" + dataroot + "/advisor_und.json")
#let report = json("/" + dataroot + "/ticket_fee_detail.json")

// ── Per-class margin lookup (5-key dict + clamp) ───────────────────────────
#let resolve-top-margin(class, num-headers) = {
  let base = (
    "A": 22mm,
    "B": 24mm,
    "C": 26mm,
    "D": 28mm,
    "E": 30mm,
  ).at(class, default: 25mm)
  let extra = calc.max(0pt, calc.min(15mm, (num-headers - 2) * 3mm))
  base + extra
}

// ── Number formatting ──────────────────────────────────────────────────────
#let add-commas(n) = {
  let s = str(n)
  let neg = s.starts-with("-")
  if neg { s = s.slice(1) }
  let parts = s.split(".")
  // `.clusters()` yields graphemes — safe to slice in 3-char chunks even
  // if the input ever contains a multi-byte character (e.g. a stray minus
  // sign from float formatting).
  let int-chars = parts.at(0).clusters()
  let dec-part = if parts.len() > 1 { "." + parts.at(1) } else { "" }
  let out = ""
  let i = int-chars.len()
  while i > 3 {
    out = "," + int-chars.slice(i - 3, i).join("") + out
    i -= 3
  }
  out = int-chars.slice(0, i).join("") + out + dec-part
  if neg { "(" + out + ")" } else { out }
}

#let fmt-money(v) = {
  if v == none { return [—] }
  let n = if type(v) == str { float(v) } else { float(v) }
  let s = add-commas(calc.round(n, digits: 2))
  text(weight: if n < 0 { "regular" } else { "regular" }, s)
}
#let fmt-price(v) = {
  if v == none { return [—] }
  add-commas(calc.round(float(v), digits: 4))
}
#let fmt-rate(v) = {
  if v == none { return [—] }
  add-commas(calc.round(float(v) * 100, digits: 4)) + "%"
}
#let fmt-basis(v) = {
  if v == none { return [—] }
  add-commas(calc.round(float(v), digits: 4)) + " bp"
}
#let fmt-quantity(v) = {
  if v == none { return [—] }
  add-commas(int(v))
}
#let fmt-date(v) = {
  if v == none { return [—] }
  v
}
#let fmt-text(v) = {
  if v == none { return [—] }
  str(v)
}

// ── Cell dispatch: every cell goes through this ────────────────────────────
#let cell-of(row, key, fmt-fn) = {
  if key in row { fmt-fn(row.at(key)) } else { fmt-fn(none) }
}

// ── Column spec ────────────────────────────────────────────────────────────
// (display name, row-key, format-fn, is-numeric)
#let columns-spec = (
  ("Ticket",       "ticket",           fmt-text,     false),
  ("Trade Date",   "trade_date",       fmt-date,     false),
  ("Security",     "security",         fmt-text,     false),
  ("Side",         "side",             fmt-text,     false),
  ("CCY",          "posting_currency", fmt-text,     false),
  ("Quantity",     "quantity",         fmt-quantity, true),
  ("Price",        "price",            fmt-price,    true),
  ("Principal",    "principal",        fmt-money,    true),
  ("Commission",   "commission",       fmt-money,    true),
  ("Net",          "principal",        fmt-money,    true),
  ("Rate",         "rate",             fmt-rate,     true),
  ("Basis",        "basis",            fmt-basis,    true),
  ("Settle",       "trade_date",       fmt-date,     false),
  ("Tick",         "ticket",           fmt-text,     false),
)

// Agg-fields used for group totals.
// Each: (row-key, column-index, format-fn).
#let agg-fields = (
  ("principal",  7, fmt-money),
  ("commission", 8, fmt-money),
  ("principal",  9, fmt-money),
)

// ── Breadcrumb state (3 levels) + per-row counter ──────────────────────────
#let grp1 = state("grp-1", "")
#let grp2 = state("grp-2", "")
#let grp3 = state("grp-3", "")
#let row-counter = counter("rows")

// ── Page setup with per-class margin ───────────────────────────────────────
#set page(
  paper: "us-letter",
  flipped: true,
  margin: (
    top: resolve-top-margin(metadata.report_class, metadata.num_headers),
    bottom: 22mm,
    left: 12mm,
    right: 12mm,
  ),
  header: context {
    // layout() measures the available width per page — blocks comemo
    // memoization of the header content because each page may differ.
    // Production code measures multiple text widths per header (title,
    // account name, date) so we mirror that with three measure passes.
    layout(size => {
      let title-w = measure([
        #text(size: 11pt, weight: "bold")[#metadata.report_title — #metadata.account_name]
      ]).width
      let date-w = measure(text(size: 8pt, metadata.report_date)).width
      let acct-w = measure(text(size: 8pt, metadata.account_number)).width
      let avail = size.width - 60mm - date-w - acct-w
      let logo = rect(width: 16mm, height: 6mm, fill: rgb("#003366"),
        stroke: none, inset: 0pt)
      let title-clipped = box(width: calc.min(title-w, avail), clip: true, [
        #text(size: 11pt, weight: "bold")[#metadata.report_title — #metadata.account_name]
      ])
      stack(dir: ltr, spacing: 4mm,
        logo, title-clipped,
        align(right + horizon, text(size: 8pt, [
          #metadata.account_number · #metadata.report_date
        ])),
      )
      // Continuation breadcrumb from state.
      v(1mm)
      let g1 = grp1.get()
      let g2 = grp2.get()
      let g3 = grp3.get()
      if g1 != "" {
        text(size: 7pt, fill: gray, [
          Continued from previous · #g1 #if g2 != "" [› #g2] #if g3 != "" [› #g3]
        ])
      }
      v(0.5mm)
      line(length: 100%, stroke: 0.4pt + gray)
      // Disclaimer paragraph below header (longer than v1).
      v(1mm)
      // Use up to 400 chars of regulatory text, or all of it if shorter.
      let reg = disclaimers.regulatory
      let reg-snippet = if reg.len() > 400 { reg.slice(0, 400) } else { reg }
      text(size: 6.5pt, fill: gray, disclaimers.report + " " + reg-snippet + "…")
    })
  },
  footer: context {
    line(length: 100%, stroke: 0.3pt + gray)
    set text(size: 6pt, fill: gray)
    // 4 disclaimer paragraphs every page (more chars than v1).
    block(spacing: 1.2pt, disclaimers.report)
    block(spacing: 1.2pt, disclaimers.regulatory)
    block(spacing: 1.2pt, disclaimers.confidentiality)
    block(spacing: 1.2pt, disclaimers.footnotes.join(" "))
    grid(columns: (1fr, 1fr, 1fr),
      align(left, [Advisor: #advisor.advisor_name (#advisor.und_code)]),
      align(center, [Row counter: #context row-counter.display()]),
      align(right, [Page #counter(page).display() / #context counter(page).final().first()]),
    )
  },
)

#set text(font: "Liberation Sans", size: 7.5pt)

// ── Heading levels for bookmarks ───────────────────────────────────────────
// Levels 1-2 visible, 3-5 hidden (size 0pt) but still in outline tree.
#show heading.where(level: 1): it => block(spacing: 4pt,
  text(size: 12pt, weight: "bold", fill: rgb("#003366"), it.body))
#show heading.where(level: 2): it => block(spacing: 3pt,
  text(size: 10pt, weight: "bold", it.body))
#show heading.where(level: 3): it => block(spacing: 0pt, text(size: 0pt, it.body))
#show heading.where(level: 4): it => block(spacing: 0pt, text(size: 0pt, it.body))
#show heading.where(level: 5): it => block(spacing: 0pt, text(size: 0pt, it.body))

#set heading(numbering: none, outlined: true, bookmarked: true)

// ── Stroke styles ──────────────────────────────────────────────────────────
#let thin-stroke = 0.4pt + rgb("#444")
#let thick-stroke = 1pt + rgb("#003366")
#let dashed-row-sep = (paint: rgb("#888"), thickness: 0.2pt, dash: "dashed")

// ── Per-cell show rule (fires on EVERY table cell) ─────────────────────────
// Wraps every cell body in a padded box. Production templates often have
// 2-3 show rules like this stacked; one is enough to materially affect cost.
#show table.cell: it => box(inset: (left: 1pt, right: 1pt), it.body)

// ── Table row builder ──────────────────────────────────────────────────────
#let build-row(row) = {
  // Per-row state: increments the running row counter shown in the footer.
  let cells = columns-spec.map(spec => {
    let (_, key, fmt-fn, _) = spec
    cell-of(row, key, fmt-fn)
  })
  cells.at(0) = [#row-counter.step()#cells.at(0)]
  // ~20% of rows get a "notes" sub-row appended — mimics detail expansions.
  let notes = if "asterisk" in row {
    (table.cell(colspan: columns-spec.len(),
      fill: rgb("#fffaf0"),
      pad(x: 4pt, y: 1pt,
        text(size: 6pt, fill: rgb("#666"),
          "Note: settled outside standard T+2 cycle. " +
          "See footnote (1) on every page for additional context regarding " +
          "the timing of this trade and any FX adjustments applied."))),)
  } else { () }
  (cells, notes)
}

// ── Group total renderer (3 levels) ────────────────────────────────────────
#let render-total(label, rows, level) = {
  // Walk agg-fields and position each sum in the correct column.
  let cells = range(columns-spec.len()).map(_ => [])
  cells.at(0) = text(weight: "bold",
    fill: if level == 1 { rgb("#003366") } else if level == 2 { rgb("#446688") } else { rgb("#666") },
    label)
  for (key, col-idx, fmt-fn) in agg-fields {
    let total = 0.0
    for row in rows {
      if key in row and row.at(key) != none {
        total += float(row.at(key))
      }
    }
    cells.at(col-idx) = text(weight: "bold",
      fill: if level == 1 { rgb("#003366") } else { rgb("#446688") },
      fmt-fn(total))
  }
  let fill-color = if level == 3 { rgb("#f0f4f8") }
    else if level == 2 { rgb("#e0e8f0") }
    else { rgb("#c8d4e4") }
  table.cell(
    colspan: columns-spec.len(),
    fill: fill-color,
    grid(columns: columns-spec.len(), align: (col, _) => {
      if columns-spec.at(col).at(3) { right } else { left }
    }, ..cells)
  )
}

// Flatten all rows under a leaf so we can compute totals.
#let leaf-rows(leaf) = leaf.rows
#let mid-rows(mid) = mid.sub_groups.map(leaf-rows).flatten()
#let lead-rows(lead) = lead.sub_groups.map(mid-rows).flatten()

// ── Build the report ───────────────────────────────────────────────────────
#heading(level: 1, [Trade Detail])

// Walk the 3-level group tree, emitting headings (for bookmarks) and
// state updates (for breadcrumbs) along the way.

#let grand-rows = report.groups.map(lead-rows).flatten()

#for lead in report.groups {
  grp1.update(lead.name)
  grp2.update("")
  grp3.update("")
  heading(level: 2, lead.name)

  let lead-flat = lead-rows(lead)

  for mid in lead.sub_groups {
    grp2.update(mid.name)
    grp3.update("")
    heading(level: 3, mid.name)

    for leaf in mid.sub_groups {
      grp3.update(leaf.name)
      heading(level: 4, leaf.name)
      // Hidden level-5 heading too (extra bookmark depth).
      heading(level: 5, leaf.code)

      // Build all rows (cells + optional notes sub-rows) for this leaf.
      let leaf-cells = ()
      for row in leaf.rows {
        let (cells, notes) = build-row(row)
        leaf-cells += cells
        leaf-cells += notes
      }

      // Table for this leaf's rows.
      table(
        columns: columns-spec.len(),
        align: (col, _) => if columns-spec.at(col).at(3) { right } else { left },
        stroke: (x, y) => {
          // Thick outer border, thin inner verticals, dashed horizontal between rows.
          (
            left: if x == 0 { thick-stroke } else { thin-stroke },
            right: if x == columns-spec.len() - 1 { thick-stroke } else { none },
            top: if y == 0 { thick-stroke } else { dashed-row-sep },
            bottom: thick-stroke,
          )
        },
        inset: 2pt,
        // Header row.
        table.header(
          ..columns-spec.map(spec =>
            table.cell(fill: rgb("#003366"),
              text(fill: white, weight: "bold", spec.at(0)))
          )
        ),
        ..leaf-cells,
      )
    }

    // Mid-level group total.
    let mid-flat = mid-rows(mid)
    block(spacing: 3pt, line(length: 100%, stroke: 0.3pt + gray))
    table(
      columns: columns-spec.len(),
      stroke: none,
      render-total("Subtotal: " + mid.name, mid-flat, 3),
    )
  }

  // Lead-level group total.
  block(spacing: 3pt, line(length: 100%, stroke: 0.5pt + rgb("#446688")))
  table(
    columns: columns-spec.len(),
    stroke: none,
    render-total("Total: " + lead.name, lead-flat, 2),
  )
}

// Grand total.
block(spacing: 4pt, line(length: 100%, stroke: 1pt + rgb("#003366")))
#table(
  columns: columns-spec.len(),
  stroke: thick-stroke,
  render-total("Grand Total", grand-rows, 1),
)

// ── Optional end-disclaimer page ───────────────────────────────────────────
#if disclaimers.end_disclaimer != "" [
  #pagebreak()
  #set page(margin: (top: 30mm, bottom: 30mm, left: 25mm, right: 25mm))
  = End-of-Report Notice
  #text(size: 9pt, disclaimers.end_disclaimer)
]
