// Single-table test: loads JSON data and renders one continuous table
// Usage: typst compile table_test.typ --input datafile=data_small.json

#set page(width: 210mm, height: 297mm, margin: 1cm)
#set text(size: 8pt)

#let datafile = sys.inputs.at("datafile", default: "data_tiny.json")
#let rows = json(datafile)

#table(
  columns: 10,
  align: left,
  // Header
  [*ID*], [*Name*], [*Email*], [*Department*], [*Role*],
  [*Salary*], [*Start Date*], [*Office*], [*Phone*], [*Status*],
  // Data rows
  ..rows.map(row => (
    row.id,
    row.name,
    row.email,
    row.department,
    row.role,
    row.salary,
    row.start_date,
    row.office,
    row.phone,
    row.status,
  )).flatten()
)
