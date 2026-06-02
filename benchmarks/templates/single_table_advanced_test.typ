// Single-table advanced test: ALL data in ONE continuous table.
// Group transitions shown as spanning header rows within the table.
// This is the worst case for memory — one table spanning thousands of pages.

#set page(
  width: 210mm,
  height: 297mm,
  margin: (top: 2cm, bottom: 2cm, left: 1.5cm, right: 1.5cm),
  header: [
    #set text(size: 8pt, fill: gray)
    #grid(
      columns: (1fr, 1fr),
      align(left)[Employee Directory — Confidential],
      align(right)[Generated #datetime.today().display()],
    )
    #line(length: 100%, stroke: 0.5pt + gray)
  ],
  footer: [
    #line(length: 100%, stroke: 0.5pt + gray)
    #set text(size: 8pt, fill: gray)
    #grid(
      columns: (1fr, 1fr),
      align(left)[Company Inc. — Internal Use Only],
      align(right)[Page #context counter(page).display() of #context counter(page).final().first()],
    )
  ],
)

#set text(size: 7.5pt, font: "Arial")

#let datafile = sys.inputs.at("datafile", default: "data_advanced_medium.json")
#let data = json(datafile)

// Title page
#align(center + horizon)[
  #text(size: 24pt, weight: "bold")[Employee Directory]
  #v(1em)
  #text(size: 14pt, fill: gray)[
    Total Employees: #str(data.total_employees) \
    Departments: #str(data.groups.len()) groups
  ]
]

#pagebreak()

// Build ALL rows for a single giant table.
// Group headers are full-width spanning rows within the table.
#let all_rows = ()
#let current_group = ""

#for group in data.groups {
  // Group header row — spans all 10 columns
  all_rows.push(
    table.cell(
      colspan: 10,
      fill: rgb("#2c3e50"),
      inset: 6pt,
    )[
      #text(fill: white, weight: "bold", size: 9pt)[#group.department]
      #h(1em)
      #text(fill: rgb("#bdc3c7"), size: 7.5pt)[#group.team — #str(group.employee_count) employees]
    ]
  )

  // Employee rows
  for emp in group.employees {
    let status_color = if emp.status == "Active" { rgb("#27ae60") }
      else if emp.status == "Remote" { rgb("#2980b9") }
      else if emp.status == "On Leave" { rgb("#e67e22") }
      else if emp.status == "Contractor" { rgb("#8e44ad") }
      else { rgb("#7f8c8d") }

    all_rows.push(text(size: 6.5pt, emp.id))
    all_rows.push([*#emp.name*])
    all_rows.push(emp.role)
    all_rows.push(emp.level)
    all_rows.push(text(size: 6.5pt, emp.salary))
    all_rows.push(emp.start_date)
    all_rows.push(emp.office)
    all_rows.push(text(fill: status_color, weight: "bold", size: 6.5pt, emp.status))
    all_rows.push(text(size: 6.5pt, emp.manager))
    all_rows.push(text(size: 6pt, fill: rgb("#666"), emp.notes))
  }

  // Group footer row
  all_rows.push(
    table.cell(
      colspan: 10,
      fill: rgb("#f0f0f0"),
      inset: 3pt,
    )[
      #set text(size: 6.5pt, fill: gray)
      #align(right)[_End of #group.team — #str(group.employee_count) records_]
    ]
  )
}

// Single continuous table with everything
#table(
  columns: (6%, 14%, 10%, 3%, 8%, 8%, 6%, 6%, 10%, 29%),
  align: left,
  stroke: 0.3pt + rgb("#bdc3c7"),
  inset: 3pt,
  fill: (_, y) => if calc.odd(y) { rgb("#f8f9fa") },
  // Column headers
  table.header(
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[ID]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Name]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Role]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Lvl]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Salary]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Start Date]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Office]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Status]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Manager]),
    table.cell(fill: rgb("#1a252f"), text(fill: white, weight: "bold", size: 7pt)[Notes]),
  ),
  // All data rows (groups + employees interleaved)
  ..all_rows,
)

// Summary page
#pagebreak()
#align(center)[
  #text(size: 16pt, weight: "bold")[Summary]
  #v(1em)
  #table(
    columns: (1fr, auto),
    align: (left, right),
    stroke: 0.5pt + gray,
    inset: 6pt,
    [*Total Employees*], [#str(data.total_employees)],
    [*Total Groups*], [#str(data.groups.len())],
    [*Report Generated*], [#datetime.today().display()],
  )
]
