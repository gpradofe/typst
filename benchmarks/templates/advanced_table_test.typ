// Advanced table test: grouped data with headers, continued markers,
// page numbers, and realistic formatting.
// Simulates a real business report PDF.

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

// Column definitions for the table
#let cols = (
  ("ID", 6%),
  ("Name", 14%),
  ("Role", 10%),
  ("Lvl", 3%),
  ("Salary", 8%),
  ("Start Date", 8%),
  ("Office", 6%),
  ("Status", 6%),
  ("Manager", 10%),
  ("Notes", 29%),
)

#let header-row = cols.map(c => table.header(table.cell(
  fill: rgb("#2c3e50"),
  text(fill: white, weight: "bold", size: 7pt, c.at(0)),
)))

// Process each group
#for (gi, group) in data.groups.enumerate() {
  // Group header
  [
    #v(0.5em)
    #block(
      width: 100%,
      fill: rgb("#ecf0f1"),
      inset: 8pt,
      radius: 2pt,
    )[
      #text(size: 10pt, weight: "bold")[#group.department]
      #h(1em)
      #text(size: 8pt, fill: gray)[#group.team — #str(group.employee_count) employees]
    ]
    #v(0.3em)
  ]

  // Group table
  table(
    columns: cols.map(c => c.at(1)),
    align: left,
    stroke: 0.3pt + rgb("#bdc3c7"),
    inset: 3pt,
    fill: (_, y) => if calc.odd(y) { rgb("#f8f9fa") },
    // Header
    ..cols.map(c => table.cell(
      fill: rgb("#2c3e50"),
      text(fill: white, weight: "bold", size: 7pt, c.at(0)),
    )),
    // Data rows
    ..group.employees.map(emp => (
      text(size: 6.5pt, emp.id),
      [*#emp.name*],
      emp.role,
      emp.level,
      text(size: 6.5pt, emp.salary),
      emp.start_date,
      emp.office,
      {
        let color = if emp.status == "Active" { rgb("#27ae60") }
          else if emp.status == "Remote" { rgb("#2980b9") }
          else if emp.status == "On Leave" { rgb("#e67e22") }
          else if emp.status == "Contractor" { rgb("#8e44ad") }
          else { rgb("#7f8c8d") }
        text(fill: color, weight: "bold", size: 6.5pt, emp.status)
      },
      text(size: 6.5pt, emp.manager),
      text(size: 6pt, fill: rgb("#666"), emp.notes),
    )).flatten(),
  )

  // Group summary
  [
    #set text(size: 7pt, fill: gray)
    #align(right)[_End of #group.team — #str(group.employee_count) records_]
    #v(0.3em)
  ]
}

// Final summary page
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
