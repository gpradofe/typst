// ULTIMATE STRESS TEST: Exercises every Typst table feature
// Features exercised:
//   table: columns (mixed sizing), rows, gutter, column-gutter, row-gutter,
//          fill (function), align (function), stroke (function w/ per-side dict),
//          inset (function)
//   table.cell: colspan, rowspan, fill, align, stroke (per-side + dash), inset,
//               breakable, x, y (explicit positioning)
//   table.header: repeat, level (multi-level headers)
//   table.footer: repeat
//   table.hline: y, start, end, stroke (dashed/dotted/custom), position
//   table.vline: x, start, end, stroke
//   Stroke features: paint, thickness, cap, join, dash (named + custom array)
//   Fill types: solid, gradient.linear, gradient.radial, gradient.conic
//   Content: bold, italic, underline, overline, strike, highlight, smallcaps,
//            sub, super, math equations, shapes (rect, circle, line, polygon),
//            nested grid, box, block, stack, rotate, scale, move, place, hide,
//            raw code, numbering, symbols, links, footnotes, lists, pad
//   Show rules: show table.cell.where(x: ...) targeting specific columns
//   Set rules: set table, set table.cell
//   Context: counter(page), state, measure
//   Counters & state: custom counters, state updates per row
//   Multiple tables per page, one table per department
// Usage: typst compile stress_test.typ --input datafile=data_stress_10k.json

#let datafile = sys.inputs.at("datafile", default: "data_stress_10k.json")
#let data = json(datafile)

// ── Global settings ──
#set page(
  width: 210mm,
  height: 297mm,
  margin: (top: 2.8cm, bottom: 2.8cm, left: 1.0cm, right: 1.0cm),
  header: [
    #set text(size: 6.5pt, fill: rgb("#555"))
    #grid(
      columns: (1fr, 1fr, 1fr),
      align(left)[
        #smallcaps[Confidential] #sym.dash.em Internal Use Only
      ],
      align(center)[
        #underline(offset: 2pt, stroke: 0.3pt + rgb("#999"))[
          #data.title
        ]
      ],
      align(right)[
        Generated: #datetime.today().display() #sym.diamond.filled
      ],
    )
    #line(length: 100%, stroke: (paint: rgb("#ccc"), thickness: 0.4pt, dash: "densely-dotted"))
  ],
  footer: [
    #line(length: 100%, stroke: (paint: rgb("#ccc"), thickness: 0.4pt, dash: "densely-dotted"))
    #set text(size: 6.5pt, fill: rgb("#555"))
    #grid(
      columns: (1fr, 1fr, 1fr),
      align(left)[#data.quarter Report #sym.bullet #data.generated_by],
      align(center)[
        Page #context counter(page).display("1")
        of #context counter(page).final().first()
        #sym.dash.em
        #context [Emp\##counter("emp-counter").display("1")]
      ],
      align(right)[
        Last Updated: #datetime.today().display()
      ],
    )
  ],
)

#set text(size: 7pt, font: "Arial")
#set par(leading: 0.5em)

// ── Custom counters and state ──
#let emp-counter = counter("emp-counter")
#let dept-state = state("current-dept", "")
#let row-budget-state = state("row-budget", 0)

// ── Color helpers ──
#let status-color(status) = {
  if status == "Active" { rgb("#27ae60") }
  else if status == "On Hold" { rgb("#f39c12") }
  else if status == "Completed" { rgb("#2980b9") }
  else if status == "At Risk" { rgb("#e74c3c") }
  else if status == "Planning" { rgb("#8e44ad") }
  else { rgb("#7f8c8d") }
}

#let perf-color(score) = {
  let s = float(score)
  if s >= 4.5 { rgb("#27ae60") }
  else if s >= 3.5 { rgb("#2ecc71") }
  else if s >= 2.5 { rgb("#f39c12") }
  else { rgb("#e74c3c") }
}

#let risk-color(level) = {
  if level == "Critical" { rgb("#c0392b") }
  else if level == "High" { rgb("#e74c3c") }
  else if level == "Medium" { rgb("#f39c12") }
  else { rgb("#27ae60") }
}

#let level-color(lvl) = {
  if lvl == "L7" { rgb("#1a5276") }
  else if lvl == "L6" { rgb("#1f618d") }
  else if lvl == "L5" { rgb("#2980b9") }
  else if lvl == "L4" { rgb("#3498db") }
  else if lvl == "L3" { rgb("#5dade2") }
  else if lvl == "L2" { rgb("#85c1e9") }
  else { rgb("#aed6f1") }
}

// ── Utilization bar with gradient fill ──
#let util-bar(pct) = {
  let p = int(pct)
  let color = if p >= 90 { rgb("#e74c3c") }
    else if p >= 75 { rgb("#f39c12") }
    else if p >= 50 { rgb("#27ae60") }
    else { rgb("#3498db") }
  box(width: 100%, height: 6pt, fill: rgb("#ecf0f1"), radius: 2pt, stroke: 0.2pt + rgb("#ddd"))[
    #place(left, box(
      width: calc.min(100%, p * 1%) * 100%,
      height: 6pt,
      fill: gradient.linear(color.lighten(30%), color, color.darken(20%)),
      radius: 2pt,
    ))
    #place(center + horizon, text(size: 4pt, fill: if p > 50 { white } else { rgb("#333") })[#str(p)%])
  ]
}

// ── Satisfaction indicator with shapes ──
#let satisfaction-dots(score) = {
  let s = calc.round(float(score))
  let full-color = rgb("#f1c40f")
  let empty-color = rgb("#ddd")
  stack(dir: ltr, spacing: 1pt,
    ..range(5).map(i => {
      circle(radius: 2pt, fill: if i < s { full-color } else { empty-color }, stroke: 0.2pt + rgb("#aaa"))
    })
  )
}

// ── Risk badge ──
#let risk-badge(level) = {
  let bg = risk-color(level).lighten(70%)
  let fg = risk-color(level).darken(20%)
  box(
    fill: bg,
    radius: 2pt,
    inset: (x: 3pt, y: 1pt),
    stroke: 0.3pt + fg,
  )[
    #text(size: 5pt, fill: fg, weight: "bold")[#level]
  ]
}

// ── Level badge with conic gradient ──
#let level-badge(lvl) = {
  let c = level-color(lvl)
  box(
    fill: gradient.conic(c.lighten(40%), c, c.darken(20%), angle: 45deg),
    radius: 3pt,
    inset: (x: 3pt, y: 1.5pt),
  )[
    #text(size: 5.5pt, fill: white, weight: "bold")[#lvl]
  ]
}

// ── Sparkline mini bar chart ──
#let mini-bars(values, max-val: 400, bar-color: rgb("#3498db")) = {
  let n = values.len()
  if n == 0 { return [] }
  let bar-w = calc.min(4pt, 20pt / n)
  box(width: bar-w * n + (n - 1) * 0.5pt, height: 10pt)[
    #for (i, v) in values.enumerate() {
      let h = calc.max(1pt, calc.min(10pt, int(v) / max-val * 10pt))
      place(
        left + bottom,
        dx: (bar-w + 0.5pt) * i,
        rect(width: bar-w, height: h, fill: bar-color, radius: (top: 0.5pt)),
      )
    }
  ]
}

// ── Title page ──
#align(center + horizon)[
  #block(
    width: 80%,
    inset: 20pt,
    radius: 8pt,
    fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50"), rgb("#34495e"), dir: ttb),
    stroke: 2pt + rgb("#2c3e50"),
  )[
    #text(size: 28pt, weight: "bold", fill: white)[#data.title]
    #v(0.5em)
    #text(size: 14pt, fill: rgb("#bdc3c7"))[#data.subtitle]
    #v(1.5em)
    #line(length: 60%, stroke: (paint: rgb("#3498db"), thickness: 1pt, dash: "dashed"))
    #v(1em)
    #text(size: 12pt, fill: rgb("#ecf0f1"))[
      Departments: #str(data.departments.len()) #sym.bar.v
      Total Employees: #str(data.departments.map(d => d.headcount).sum()) #sym.bar.v
      Budget: #str(data.departments.map(d => d.budget).join(", "))
    ]
    #v(1.5em)
    #set text(size: 8pt, fill: rgb("#95a5a6"))
    This report contains detailed workforce analytics, project tracking,
    budget utilization, risk assessment, and performance metrics across all
    organizational units. Generated with maximum typographic complexity.
    #v(0.5em)
    $"Total Budget" = sum_(i=1)^(n) B_i quad "where" n = #str(data.departments.len())$
  ]
]
#pagebreak()

// ── Table of Contents ──
#align(center)[
  #text(size: 16pt, weight: "bold", fill: rgb("#2c3e50"))[Table of Contents]
]
#v(1em)
#table(
  columns: (auto, 1fr, auto),
  align: (right, left, right),
  stroke: none,
  inset: (x: 8pt, y: 4pt),
  fill: (_, y) => if calc.odd(y) { rgb("#f8f9fa") },
  ..data.departments.enumerate().map(((i, d)) => (
    text(size: 8pt, fill: rgb("#2980b9"), weight: "bold")[#str(i + 1).],
    [#text(size: 9pt)[#d.name Department] #box(width: 1fr, repeat[.])],
    text(size: 8pt, fill: rgb("#888"))[#str(d.headcount) employees],
  )).flatten(),
)
#pagebreak()

// ───────────────────────────────────────────────────
// Per-department tables
// ───────────────────────────────────────────────────

#for (dept_idx, dept) in data.departments.enumerate() {
  if dept_idx > 0 { pagebreak() }

  // Update state
  dept-state.update(dept.name)

  // ── Department header with radial gradient background ──
  block(
    width: 100%,
    inset: 8pt,
    radius: 4pt,
    fill: gradient.radial(rgb("#2c3e50"), rgb("#34495e"), rgb("#1a252f"), focal-center: (30%, 30%)),
    stroke: (
      left: 3pt + rgb("#3498db"),
      rest: 0.5pt + rgb("#555"),
    ),
  )[
    #text(size: 14pt, weight: "bold", fill: white)[
      #numbering("I.", dept_idx + 1) #dept.name Department
    ]
    #h(1fr)
    #text(size: 8pt, fill: rgb("#bdc3c7"))[
      Manager: #underline[#dept.manager]
    ]
    #v(0.3em)
    #grid(
      columns: (1fr, 1fr, 1fr, 1fr),
      gutter: 1em,
      text(size: 7pt, fill: rgb("#ecf0f1"))[
        Budget: *#dept.budget*
      ],
      text(size: 7pt, fill: rgb("#ecf0f1"))[
        Headcount: *#str(dept.headcount)*
      ],
      text(size: 7pt, fill: rgb("#ecf0f1"))[
        Teams: *#str(dept.teams.len())*
      ],
      text(size: 7pt, fill: rgb("#ecf0f1"))[
        Projects: *#str(dept.teams.map(t => t.projects.len()).sum())*
      ],
    )
  ]
  v(0.3em)

  // ── Build rows ──
  let all_rows = ()

  for (team_idx, team) in dept.teams.enumerate() {
    // ── Team header row (colspan=14) with gradient fill ──
    all_rows.push(
      table.cell(
        colspan: 14,
        fill: gradient.linear(rgb("#2c3e50"), rgb("#34495e")),
        inset: (x: 6pt, y: 4pt),
        stroke: (bottom: 1.5pt + rgb("#3498db")),
      )[
        #text(fill: white, weight: "bold", size: 8pt)[
          #sym.arrow.r Team: #team.name
        ]
        #h(1em)
        #text(fill: rgb("#bdc3c7"), size: 7pt)[
          Lead: #emph[#team.lead] #sym.bar.v #team.location
        ]
        #h(1fr)
        #text(fill: rgb("#95a5a6"), size: 6pt)[
          #str(team.projects.len()) projects #sym.bullet
          #str(team.projects.map(p => p.employees.len()).sum()) members
        ]
      ]
    )
    // Decorative hline after team header
    all_rows.push(
      table.hline(stroke: (paint: rgb("#3498db"), thickness: 0.5pt, dash: "dashed"))
    )

    for (proj_idx, project) in team.projects.enumerate() {
      // ── Project sub-header (colspan=14) ──
      all_rows.push(
        table.cell(
          colspan: 14,
          fill: if calc.odd(proj_idx) { rgb("#eaf2f8") } else { rgb("#fef9e7") },
          inset: (x: 8pt, y: 3pt),
          stroke: (
            left: 3pt + status-color(project.status),
            bottom: 0.5pt + rgb("#ccc"),
            rest: none,
          ),
        )[
          #grid(
            columns: (auto, 1fr, auto, auto, auto, auto, auto),
            gutter: 0.8em,
            align: (left, left, center, center, right, right, right),
            stack(dir: ltr, spacing: 3pt,
              text(weight: "bold", size: 7.5pt)[#project.name],
              text(size: 6pt, fill: rgb("#888"))[ (Sprint \##str(project.sprint))],
            ),
            [],
            box(
              fill: status-color(project.status).lighten(70%),
              radius: 2pt,
              inset: (x: 4pt, y: 1pt),
              stroke: 0.3pt + status-color(project.status),
            )[#text(size: 6pt, fill: status-color(project.status), weight: "bold")[#project.status]],
            text(size: 6pt)[#project.start_date #sym.arrow.r #project.end_date],
            text(size: 6pt, fill: rgb("#888"))[
              Risk: #text(fill: risk-color(if project.risk_score > 7 { "Critical" } else if project.risk_score > 5 { "High" } else if project.risk_score > 3 { "Medium" } else { "Low" }), weight: "bold")[#str(project.risk_score)]
            ],
            text(size: 6pt)[Vel: #str(project.velocity)],
            text(size: 6pt)[
              Budget: #project.budget_allocated
              (#highlight(fill: if project.completion > 80 { rgb("#d5f5e3") } else { rgb("#fdebd0") })[#project.budget_spent] spent)
            ],
          )
        ]
      )
      // Vline decoration at project boundary
      all_rows.push(
        table.hline(
          start: 0, end: 1,
          stroke: (paint: status-color(project.status), thickness: 1pt, cap: "round"),
        )
      )

      // ── Employee data rows (14 columns) ──
      for (emp_idx, emp) in project.employees.enumerate() {
        // Step the employee counter
        emp-counter.step()

        let row_fill = if calc.even(emp_idx) { none }
          else { rgb("#f8f9fa") }

        // Col 0: ID with monospace
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #text(size: 5.5pt, fill: rgb("#888"), font: "Consolas")[#emp.id]
        ])

        // Col 1: Name (bold) + email (small, italic)
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #stack(spacing: 0.5pt,
            text(weight: "bold", size: 6.5pt)[#emp.name],
            text(size: 4.5pt, fill: rgb("#999"), style: "italic")[#emp.email],
          )
        ])

        // Col 2: Role + Level badge
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #stack(dir: ltr, spacing: 2pt,
            text(size: 6pt)[#emp.role],
            level-badge(emp.level),
          )
        ])

        // Col 3: Location
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #text(size: 5.5pt, fill: rgb("#666"))[#sym.diamond.small #emp.location]
        ])

        // Col 4: Salary + bonus
        all_rows.push(table.cell(
          fill: row_fill,
          inset: (x: 2pt, y: 1.5pt),
          align: right,
        )[
          #text(size: 6.5pt)[#emp.salary]
          #if emp.bonus_pct > 0 [
            #text(size: 4.5pt, fill: rgb("#27ae60"))[ +#str(emp.bonus_pct)%]
          ]
        ])

        // Col 5: Utilization bar
        all_rows.push(table.cell(fill: row_fill, inset: (x: 1pt, y: 2pt))[
          #util-bar(emp.utilization)
        ])

        // Col 6: Hours + overtime with overline if high
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt), align: center)[
          #text(size: 6pt)[#str(emp.hours_logged)h]
          #if emp.overtime_hours > 20 [
            #text(size: 4.5pt, fill: rgb("#e74c3c"))[
              #overline(stroke: 0.3pt + rgb("#e74c3c"))[+#str(emp.overtime_hours)OT]
            ]
          ] else if emp.overtime_hours > 0 [
            #text(size: 4.5pt, fill: rgb("#888"))[+#str(emp.overtime_hours)]
          ]
        ])

        // Col 7: Tasks fraction with mini progress rect
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt), align: center)[
          #let total = emp.tasks_completed + emp.tasks_pending
          #let pct = if total > 0 { calc.round(emp.tasks_completed / total * 100) } else { 0 }
          #stack(spacing: 1pt,
            text(size: 6pt)[#str(emp.tasks_completed)/#str(total)],
            rect(width: 20pt, height: 2.5pt, fill: rgb("#ecf0f1"), radius: 1pt, stroke: 0.1pt + rgb("#ddd"))[
              #place(left, rect(
                width: pct * 1% * 20pt,
                height: 2.5pt,
                fill: if pct >= 80 { rgb("#27ae60") } else if pct >= 50 { rgb("#f39c12") } else { rgb("#e74c3c") },
                radius: 1pt,
              ))
            ],
          )
        ])

        // Col 8: Performance score with color
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt), align: center)[
          #text(
            fill: perf-color(str(emp.performance_score)),
            weight: "bold",
            size: 7pt,
          )[#str(emp.performance_score)]
        ])

        // Col 9: Satisfaction dots
        all_rows.push(table.cell(fill: row_fill, inset: (x: 1pt, y: 2pt), align: center)[
          #satisfaction-dots(emp.satisfaction)
        ])

        // Col 10: Status with colored text
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt), align: center)[
          #text(size: 5.5pt, fill: status-color(emp.status), weight: "bold")[#emp.status]
        ])

        // Col 11: Risk badge
        all_rows.push(table.cell(fill: row_fill, inset: (x: 1pt, y: 1.5pt), align: center)[
          #risk-badge(emp.risk_level)
        ])

        // Col 12: Certification (smallcaps) + hire date
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #if emp.certification != "" [
            #text(size: 5pt)[#smallcaps[#emp.certification]]
            #linebreak()
          ]
          #text(size: 4.5pt, fill: rgb("#aaa"))[#emp.hire_date]
        ])

        // Col 13: Notes (truncated, with metrics)
        all_rows.push(table.cell(fill: row_fill, inset: (x: 2pt, y: 1.5pt))[
          #text(size: 5pt, fill: rgb("#666"))[#emp.notes]
          #v(0.5pt)
          #text(size: 4pt, fill: rgb("#bbb"))[
            T:#str(emp.tickets_resolved)
            #sym.dot.c CR:#str(emp.code_reviews)
            #sym.dot.c M:#str(emp.meetings_attended)
          ]
        ])
      }

      // ── Project summary row (colspan=14) ──
      all_rows.push(
        table.cell(
          colspan: 14,
          fill: rgb("#fafafa"),
          inset: (x: 8pt, y: 2pt),
          stroke: (top: (paint: rgb("#ddd"), thickness: 0.3pt, dash: "dotted"), rest: none),
        )[
          #set text(size: 5.5pt, fill: rgb("#888"))
          #grid(
            columns: (1fr, auto),
            align(left)[
              #emph[#project.name] #sym.bar.v
              #str(project.employees.len()) employees #sym.bar.v
              Completion: *#str(project.completion)%* #sym.bar.v
              Priority: #text(
                fill: if project.priority == "Critical" { rgb("#c0392b") }
                  else if project.priority == "High" { rgb("#e67e22") }
                  else { rgb("#888") },
                weight: if project.priority == "Critical" { "bold" } else { "regular" },
              )[#project.priority]
            ],
            align(right)[
              $"Completion" = #str(project.completion) / 100 = #{ let c = float(project.completion) / 100; str(calc.round(c, digits: 2)) }$
            ],
          )
        ]
      )
    }

    // ── Team summary row ──
    all_rows.push(
      table.cell(
        colspan: 14,
        fill: gradient.linear(rgb("#ebedef"), rgb("#d5d8dc")),
        inset: (x: 6pt, y: 3pt),
        stroke: (
          top: 1pt + rgb("#bbb"),
          bottom: 1pt + rgb("#bbb"),
          rest: none,
        ),
      )[
        #set text(size: 6.5pt, weight: "bold", fill: rgb("#555"))
        #grid(
          columns: (auto, 1fr, auto),
          text[#sym.checkmark.heavy End of #team.name],
          [],
          text[
            #str(team.projects.map(p => p.employees.len()).sum()) employees
            #sym.bar.v
            #str(team.projects.len()) projects
          ],
        )
      ]
    )
    // Decorative double hline after team
    all_rows.push(
      table.hline(stroke: (paint: rgb("#95a5a6"), thickness: 0.8pt, dash: (3pt, 2pt, 1pt, 2pt)))
    )
  }

  // ── Department total row ──
  all_rows.push(
    table.cell(
      colspan: 14,
      fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50"), rgb("#34495e")),
      inset: (x: 8pt, y: 5pt),
      stroke: (
        top: 2pt + rgb("#3498db"),
        bottom: 2pt + rgb("#2c3e50"),
        rest: none,
      ),
    )[
      #text(fill: white, weight: "bold", size: 8pt)[
        #sym.square.filled #dept.name Total: #str(dept.headcount) employees
        #h(1fr)
        Budget: #dept.budget
      ]
    ]
  )

  // ── The main table ──
  table(
    columns: (
      4%,   // ID
      11%,  // Name+Email
      10%,  // Role+Level
      5%,   // Location
      6%,   // Salary
      9%,   // Utilization
      5%,   // Hours
      5%,   // Tasks
      3.5%, // Score
      5%,   // Satisfaction
      4.5%, // Status
      4%,   // Risk
      7%,   // Cert+Date
      21%,  // Notes+Metrics
    ),
    align: (x, y) => {
      if y == 0 { center }
      else if x == 4 { right }
      else if x == 6 or x == 7 or x == 8 or x == 9 or x == 10 or x == 11 { center }
      else { left }
    },
    stroke: (x, y) => {
      if y == 0 {
        (bottom: (paint: rgb("#2c3e50"), thickness: 1.5pt, cap: "round"))
      } else {
        (
          bottom: (paint: rgb("#e0e0e0"), thickness: 0.25pt),
          right: if x < 13 { (paint: rgb("#eee"), thickness: 0.2pt, dash: "dotted") },
        )
      }
    },
    inset: (x, y) => {
      if y == 0 { (x: 3pt, y: 4pt) }
      else { (x: 2pt, y: 1.5pt) }
    },
    fill: (x, y) => {
      if y == 0 { gradient.linear(rgb("#1a252f"), rgb("#2c3e50")) }
    },
    row-gutter: 0pt,

    // ── Multi-level repeating header ──
    table.header(
      // Level 1: main column headers
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[ID]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Name / Email]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Role / Level]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Loc]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Salary]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Utilization]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Hours]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Tasks]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Perf]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Satis.]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Status]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Risk]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Cert / Hire]
      ],
      table.cell(fill: gradient.linear(rgb("#1a252f"), rgb("#2c3e50")))[
        #text(fill: white, weight: "bold", size: 6pt)[Notes & Metrics]
      ],
      // Decorative hline below header
      table.hline(stroke: (paint: rgb("#3498db"), thickness: 0.8pt, dash: "dashed")),
    ),

    // Data rows
    ..all_rows,

    // ── Repeating footer ──
    table.footer(
      table.cell(
        colspan: 14,
        fill: rgb("#f0f0f0"),
        inset: 3pt,
        stroke: (top: 0.5pt + rgb("#ccc")),
      )[
        #set text(size: 5.5pt, fill: rgb("#999"))
        #grid(
          columns: (1fr, 1fr, 1fr),
          align(left)[#dept.name Department #sym.dash.em Continued],
          align(center)[
            #context [Row counter: #emp-counter.display("1")]
          ],
          align(right)[
            Page #context counter(page).display("1") #sym.bar.v
            #datetime.today().display()
          ],
        )
      ],
    ),
  )
}

// ───────────────────────────────────────────────────
// Summary page
// ───────────────────────────────────────────────────
#pagebreak()

#align(center)[
  #text(size: 18pt, weight: "bold", fill: rgb("#2c3e50"))[
    #underline(offset: 3pt, stroke: 1pt + rgb("#3498db"))[Organization Summary]
  ]
]
#v(1em)

// Summary table with show rule
#{
  show table.cell.where(y: 0): it => {
    set text(fill: white, weight: "bold", size: 8pt)
    it
  }
  show table.cell.where(x: 0): it => {
    set text(weight: "bold")
    it
  }

  table(
    columns: (1fr, auto, auto, auto, auto),
    align: (left, right, right, right, center),
    stroke: (x, y) => (
      bottom: if y == 0 { 2pt + rgb("#2c3e50") } else { 0.5pt + rgb("#ddd") },
      right: if x < 4 { 0.5pt + rgb("#eee") },
    ),
    inset: (x: 8pt, y: 5pt),
    fill: (_, y) => if y == 0 { gradient.linear(rgb("#1a252f"), rgb("#2c3e50")) }
      else if calc.odd(y) { rgb("#f8f9fa") },

    // Column headers
    [Department], [Headcount], [Budget], [Teams], [Avg Team Size],

    // Data rows
    ..data.departments.map(d => {
      let avg = calc.round(d.headcount / d.teams.len())
      (
        d.name,
        str(d.headcount),
        d.budget,
        str(d.teams.len()),
        str(avg),
      )
    }).flatten(),

    // Total row
    table.hline(stroke: 1.5pt + rgb("#2c3e50")),
    table.cell(fill: rgb("#2c3e50"))[
      #text(fill: white, weight: "bold")[TOTAL]
    ],
    table.cell(fill: rgb("#2c3e50"))[
      #text(fill: white, weight: "bold")[
        #str(data.departments.map(d => d.headcount).sum())
      ]
    ],
    table.cell(fill: rgb("#2c3e50"))[
      #text(fill: white, weight: "bold")[--]
    ],
    table.cell(fill: rgb("#2c3e50"))[
      #text(fill: white, weight: "bold")[
        #str(data.departments.map(d => d.teams.len()).sum())
      ]
    ],
    table.cell(fill: rgb("#2c3e50"))[
      #text(fill: white, weight: "bold")[--]
    ],
  )
}

#v(1.5em)

// ── Final statistics with math ──
#align(center)[
  #block(
    width: 70%,
    inset: 12pt,
    radius: 4pt,
    fill: rgb("#fafafa"),
    stroke: 0.5pt + rgb("#ddd"),
  )[
    #set text(size: 8pt)
    #grid(
      columns: (1fr, 1fr),
      gutter: 1em,
      [
        *Report Statistics*
        - Total departments: #str(data.departments.len())
        - Total employees: #str(data.departments.map(d => d.headcount).sum())
        - Total teams: #str(data.departments.map(d => d.teams.len()).sum())
        - Total projects: #str(data.departments.map(d => d.teams.map(t => t.projects.len()).sum()).sum())
      ],
      [
        *Generation Info*
        - Date: #datetime.today().display()
        - Pages: #context counter(page).final().first()
        - Employees counted: #context emp-counter.display("1")
        - System: #data.generated_by
      ],
    )
    #v(0.5em)
    #align(center)[
      $overline("Performance") = 1/N sum_(i=1)^(N) p_i quad "where" N = #str(data.departments.map(d => d.headcount).sum())$
    ]
  ]
]
