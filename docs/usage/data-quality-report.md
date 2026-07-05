# Data Quality Report

**Analyse > Data quality report...** opens a new tab that scores each column of
the active table, so you can see at a glance where the data needs cleaning.

## What it shows

One row per source column, with these metric columns (hover any header for a
full explanation):

| Column                  | Meaning                                                                         |
|-------------------------|---------------------------------------------------------------------------------|
| `null_percentage`       | Percentage of missing (null) values.                                            |
| `distinct_ratio`        | Distinct values divided by non-null values (1.0 = all unique).                  |
| `outlier_count`         | Number of numeric outliers (IQR method, as in Detect outliers).                 |
| `pii_flag` / `pii_kind` | Whether the column looks like personal data, and what kind (reuses Detect PII). |
| `type_consistency`      | Share of values that match the column's declared type.                          |
| `score`                 | Overall 0-100 quality score for the column.                                     |

The **score** combines completeness, uniqueness and type consistency, with a
small penalty for outliers. The overall table score (the average of the column
scores) appears in the status bar when the report opens.

The report is an ordinary table tab: sort it, filter it, or save it like any
other file. Re-run the report after cleaning to watch the score improve.
