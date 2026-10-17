# Analyse

Statistics, frequencies, time series, charts and reports from the table in front of you.

![Analyse](../../assets/screenshots/chart-tab-overview.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                     | In the app                                        | Command line                                          | MCP tool                                                                                                               |
|----------------------------------|---------------------------------------------------|-------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|
| Summary statistics per column    | [Summary](../../usage/summary.md)                 | [`--describe`](../../cli/describe.md)                 | [`profile`](../../mcp/tools/profile.md)                                                                                |
| Count values in a column         | [Value Frequency](../../usage/value-frequency.md) | -                                                     | [`value_frequency`](../../mcp/tools/value_frequency.md)                                                                |
| Correlate numeric columns        | [Correlation](../../usage/correlation.md)         | -                                                     | [`correlation`](../../mcp/tools/correlation.md)                                                                        |
| Time buckets and rolling windows | [Time Series](../../usage/time-series.md)         | [`--resample` / `--rolling`](../../cli/timeseries.md) | [`resample_timeseries`](../../mcp/tools/resample_timeseries.md), [`rolling_window`](../../mcp/tools/rolling_window.md) |
| Draw a chart                     | [Chart](../../usage/chart.md)                     | -                                                     | -                                                                                                                      |
| Show a trend and a forecast      | [Trend and forecast](../../usage/forecast.md)     | [`--forecast`](../../cli/forecast.md)                 | [`forecast`](../../mcp/tools/forecast.md)                                                                              |
| Write an HTML report             | [Report](../../usage/report.md)                   | [`--report`](../../cli/report.md)                     | [`create_report`](../../mcp/tools/create_report.md)                                                                    |
| Look at a file's internals       | [File Internals](../../usage/file-internals.md)   | [`--describe`](../../cli/describe.md)                 | [`describe_file`](../../mcp/tools/describe_file.md)                                                                    |
| Find overlapping time spans      | [Timeline](../../usage/view-modes/timeline.md)    | [`--overlaps`](../../cli/overlaps.md)                 | [`find_overlaps`](../../mcp/tools/find_overlaps.md)                                                                    |
