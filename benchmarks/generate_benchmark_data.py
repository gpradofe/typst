"""
Generate JSON test data files for comprehensive benchmarking.
Produces both simple and advanced (grouped) formats at all sizes.

Usage:
    python generate_benchmark_data.py              # Generate all sizes
    python generate_benchmark_data.py 100 1000     # Specific sizes only
"""
import os
import sys
import time
import random

random.seed(42)

# All benchmark sizes
ALL_SIZES = [100, 1_000, 10_000, 50_000, 100_000, 300_000, 600_000, 1_200_000]

# ── Simple table row (matches generate_data.py format) ─────────────────────

def write_simple_row(f, i):
    dept = i % 20
    role = i % 15
    salary = 50000 + (i * 7) % 100000
    month = (i % 12) + 1
    day = (i % 28) + 1
    building = chr(65 + i % 5)
    floor = i % 10
    phone = i % 1000
    status = ["Active", "On Leave", "Remote", "Hybrid"][i % 4]
    f.write(
        f'{{"id": "{i}", '
        f'"name": "Employee Name {i}", '
        f'"email": "employee{i}@company.com", '
        f'"department": "Dept-{dept}", '
        f'"role": "Role-{role}", '
        f'"salary": "{salary}", '
        f'"start_date": "2020-{month:02d}-{day:02d}", '
        f'"office": "Building-{building} Floor-{floor}", '
        f'"phone": "+1-555-{phone:04d}", '
        f'"status": "{status}"}}'
    )


def generate_simple(path, count):
    t0 = time.time()
    with open(path, "w", buffering=1024 * 1024) as f:
        f.write("[")
        for i in range(count):
            if i > 0:
                f.write(", ")
            write_simple_row(f, i)
        f.write("]")
    elapsed = time.time() - t0
    size_mb = os.path.getsize(path) / (1024 * 1024)
    print(f"  simple {count:>10,} rows -> {size_mb:>8.1f} MB  ({elapsed:.1f}s)")
    return size_mb


# ── Advanced table row (matches generate_advanced_data.py format) ──────────

DEPARTMENTS = [
    "Engineering", "Sales", "Marketing", "Finance", "Operations",
    "Human Resources", "Legal", "Product", "Customer Success", "Research",
    "IT Infrastructure", "Quality Assurance", "Business Development",
    "Data Science", "Security", "Design", "Support", "Analytics",
    "Procurement", "Compliance"
]
TEAMS = [
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
    "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi"
]
ROLES = [
    "Engineer", "Senior Engineer", "Staff Engineer", "Principal Engineer",
    "Manager", "Senior Manager", "Director", "VP", "Analyst", "Specialist",
    "Coordinator", "Lead", "Architect", "Consultant", "Associate"
]
STATUSES = ["Active", "On Leave", "Remote", "Hybrid", "Contractor", "Part-Time"]
OFFICES = ["NYC", "SF", "London", "Berlin", "Tokyo", "Sydney", "Toronto", "Singapore"]


def write_advanced_row(f, i, dept, team_name):
    role = ROLES[i % len(ROLES)]
    level = (i % 8) + 1
    salary_val = 50000 + (i * 7) % 150000
    year = 18 + i % 7
    month = (i % 12) + 1
    day = (i % 28) + 1
    office = OFFICES[i % len(OFFICES)]
    phone = i % 10000
    status = STATUSES[i % len(STATUSES)]
    manager_id = (i // 20) % 500
    review_year = 2024 + i % 3
    rating = ["Exceeds", "Meets", "Developing"][i % 3]
    f.write(
        f'{{"id": "{i}", '
        f'"department": "{dept}", '
        f'"team": "{team_name}", '
        f'"name": "Employee {i:06d}", '
        f'"email": "emp{i}@company.com", '
        f'"role": "{role}", '
        f'"level": "{level}", '
        f'"salary": "${salary_val:,}", '
        f'"start_date": "20{year}-{month:02d}-{day:02d}", '
        f'"office": "{office}", '
        f'"phone": "+1-555-{phone:04d}", '
        f'"status": "{status}", '
        f'"manager": "Manager {manager_id:04d}", '
        f'"notes": "Performance review {review_year}. Rating: {rating}."}}'
    )


def generate_advanced(path, total_rows):
    t0 = time.time()
    rng = random.Random(42)
    groups = []
    i = 0
    while i < total_rows:
        dept = DEPARTMENTS[i % len(DEPARTMENTS)]
        team = TEAMS[(i // 50) % len(TEAMS)]
        team_name = f"{dept} - {team}"
        group_size = rng.randint(15, 80)
        group_size = min(group_size, total_rows - i)
        groups.append((i, i + group_size, dept, team_name))
        i += group_size

    with open(path, "w", buffering=1024 * 1024) as f:
        f.write('{"groups": [')
        for gi, (start, end, dept, team_name) in enumerate(groups):
            if gi > 0:
                f.write(", ")
            emp_count = end - start
            f.write(
                f'{{"department": "{dept}", '
                f'"team": "{team_name}", '
                f'"employee_count": {emp_count}, '
                f'"employees": ['
            )
            for j in range(start, end):
                if j > start:
                    f.write(", ")
                write_advanced_row(f, j, dept, team_name)
            f.write("]}")
        f.write(f'], "total_employees": {total_rows}}}')

    elapsed = time.time() - t0
    size_mb = os.path.getsize(path) / (1024 * 1024)
    print(f"  advanced {total_rows:>10,} rows -> {size_mb:>8.1f} MB  ({elapsed:.1f}s)  ({len(groups)} groups)")
    return size_mb


# ── Main ───────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    out_dir = os.path.dirname(os.path.abspath(__file__))

    if len(sys.argv) > 1:
        sizes = [int(x.replace(",", "").replace("_", "")) for x in sys.argv[1:]]
    else:
        sizes = ALL_SIZES

    print(f"Generating benchmark data for {len(sizes)} sizes...")
    print()

    for count in sizes:
        label = f"{count // 1000}k" if count >= 1000 else str(count)

        # Simple format
        simple_path = os.path.join(out_dir, f"data_{label}.json")
        if not os.path.exists(simple_path):
            generate_simple(simple_path, count)
        else:
            size_mb = os.path.getsize(simple_path) / (1024 * 1024)
            print(f"  simple {count:>10,} rows -> {size_mb:>8.1f} MB  (exists)")

        # Advanced format
        adv_path = os.path.join(out_dir, f"data_advanced_{label}.json")
        if not os.path.exists(adv_path):
            generate_advanced(adv_path, count)
        else:
            size_mb = os.path.getsize(adv_path) / (1024 * 1024)
            print(f"  advanced {count:>10,} rows -> {size_mb:>8.1f} MB  (exists)")

    print("\nDone!")
