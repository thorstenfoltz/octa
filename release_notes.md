A release of fixes, most of them for database connections. PostgreSQL servers
with a certificate of their own, such as a Kubernetes cluster reached through
`kubectl port-forward`, now connect the way they do in psql and DBeaver, and
every PostgreSQL, Redshift and MySQL connection gets an SSL mode, a CA file
and a Connect via address. PostgreSQL through an SSH jump host connects, MySQL
connections are encrypted, and a failed Attach no longer shows the database
password. Besides the fixes, an open table can be saved to the cloud or an
SFTP server, DuckDB and SQLite files can be saved as connections, the text
editors take several carets, value shapes read by word, and earlier versions
can be kept wherever you like.

## Breaking changes

These change what an existing setup does. None of them touches your files.

- **The default SSL mode no longer checks the server's certificate.** A
  PostgreSQL or Redshift connection saved before this release gets prefer,
  as in psql and DBeaver. It encrypts whenever the server can, and accepts a
  private or self-signed certificate. Octa used to check the certificate even
  then, which refused every server signed by its own authority. To keep the
  check, set the connection's **SSL mode** to **verify-full**.
- **Value shapes go by word.** `Anna Schmidt` now has the shape `Aa Aa`
  instead of `Aaaa Aaaaaaa`, in the funnel, the column filter window, the
  Quality Report, `--shapes` and `value_shapes`. The **Words / Exact** switch
  brings back the letter-by-letter shapes, as do `--shapes-exact` and
  `exact: true` for scripts and agents.

## Fixes

- **PostgreSQL servers with their own certificate were refused** (issue
  #87). A CloudNativePG cluster through `kubectl port-forward`, PostgreSQL on
  Debian or Ubuntu with its default certificate, or a server signed by a
  company authority failed with "UnknownIssuer", while psql and DBeaver
  connected. Octa now does what they do. See the new SSL settings below.
- **PostgreSQL through an SSH jump host never connected.** The connection
  failed with "invalid number of ports" before it reached the server.
- **MySQL and MariaDB connections that sign in with a password were never
  encrypted.** They are now, whenever the server offers it. A server that
  offers no encryption still connects as before.
- **A failed Attach showed the database password.** When attaching a
  PostgreSQL or MySQL connection in the SQL panel, or a fast copy between
  servers, failed, the error repeated the whole connection string, password
  included. It now reads `password=***`, so the message is safe to copy into
  a report.
- **A MySQL password with a space, a quote, a backslash or `=` never
  attached.** Attach and the fast copy between servers failed for such a
  password. They now pass it through as it is.
- **Write result to DB**
  - A result from a query run on a connection, on Databricks for instance,
    could not go into a DuckDB or SQLite file. Octa ran the query again
    locally, where its tables do not exist. It now writes the rows the server
    returned.
  - A result shown in pages wrote only the pages loaded so far. It now writes
    every row.
  - A failed write said only "writing result to ..." and left out the
    reason. The message now gives the whole chain, the server's own words
    included.
- **A CSV starting with a byte order mark counted as malformed**, and Octa
  offered to repair it. Spreadsheet programs write that mark on purpose, and
  Octa now reads it without a prompt.
- **Copy to... onto another connection kept the source's path.** Copying a
  file from a bucket onto an SFTP server proposed the bucket's path, such as
  `data/sales.csv`, which on the server means a folder at its very top that is
  usually not writable. The path now starts in the target connection's start
  folder, with the file's name.
- **Saving a big file back to the cloud read it into memory whole**, and so
  did `--cloud-put`. Both now send it in blocks.

## What's new

### Save a table to the cloud or SFTP

**File > Save to cloud/SFTP...** writes the table in the active tab as a file
into a bucket, onto an SFTP server or into a WebDAV folder, whatever the tab
came from. Right-click a folder under **Cloud/SFTP connections** in the
sidebar and pick **Save the active tab here...** to start there. The ending of the name decides the format, the
Save as questions follow when they apply, and the upload runs in the
background. The tab stays as it was.

### SSL mode, CA file and Connect via

PostgreSQL, Redshift and MySQL / MariaDB connections have three new fields
under **Settings > Databases**.

- **SSL mode**, in the words psql and the mysql client use. disable never
  encrypts. prefer, the default, encrypts when the server can. require
  always encrypts. verify-ca also checks who signed the certificate, and
  verify-full (verify_identity on MySQL) checks that and the server name.
  Every mode explains itself under the drop-down.
- **CA file**, a PEM file on this computer with the authority that signed
  the server's certificate, for the two verify modes. Pick it with
  **Browse...**.
- **Connect via**, an IP address to dial instead of Host. Host then only
  names the certificate, so verify-full works through a port-forward, with
  Host set to the cluster's name and Connect via to `127.0.0.1`.

A sign-in with a token (AWS IAM, Microsoft Entra, Google Cloud SQL IAM)
always travels encrypted, whatever the mode says. Attach and the fast copy
between servers follow the same settings. On the command line,
`--add-connection` takes them as `sslmode=`, `sslrootcert=` and `hostaddr=`,
the names psql uses. MySQL has no verify_ca for now, since its driver cannot
yet check who signed a certificate without also checking the name.

### DuckDB and SQLite files as connections

Pick **DuckDB** or **SQLite** as the engine under **Settings > Databases** and
give the file. The file then works like a server: browse its tables in the
sidebar, open and edit them (with **Allow writes**), run SQL on it, attach it
in the SQL panel, save a tab into it, and write a query result to it. Octa
opens the file for each action and closes it straight after, so other
programs can use it in between. A file that is not there is refused, never
created. On the command line it is
`--add-connection 'kind=duckdb,name=NAME,file=PATH'` (or `kind=sqlite`), and
the database MCP tools read these connections too.

### Several carets in the text editors

The Raw view, Markdown Edit and the SQL editor take more than one caret, as in
VS Code. **Alt+click** adds one, **Shift+Alt+Up / Down** adds one on the line
above or below, and **Shift+Alt+I** puts one at the end of every marked line.
Typing, deleting and the arrow keys then work at every caret, and a paste with
one line per caret gives each caret its own line. **Esc** goes back to one.
The keys can be changed under **Settings > Shortcuts > Text editors**, and the
click under **Settings > Search & Editor > Add a caret with** (Alt+click,
Ctrl+click for desktops where Alt+click moves the window, or Off).

### Value shapes by word or letter by letter

Value shapes now write a run of letters of one kind once, so every
capitalised name has the same shape and the one in lower case stands out. The
**Words / Exact** switch in the funnel and the column filter window goes back
to letter by letter, for codes where the number of letters matters, and Octa
remembers the choice.

### Keep earlier versions where you like

**Settings > Files > Keep them in** decides where **Keep earlier versions**
puts its copies. The choices are Octa's own folder as before, one folder you
choose, a `.octa-versions` folder next to each file, or right beside the
file. Versions kept in an earlier place are still listed.

### Write a result into a DuckDB or SQLite file

**Write result to DB** has **Existing database...** and **New database...**
for a file target, and a line under the path says which of the two it is. A
new file is created when you press **Write**.

## Changed

- **The File menu lists its save entries in one order.** Save, Save as, Save
  with password, Save to cloud/SFTP and Save to database come first, then
  Save database changes as SQL on a database tab and Earlier versions.
  **Settings > Shortcuts** lists them the same way.
- **Cloud connections is now Cloud/SFTP connections**, in the File menu and
  in **Settings > Panels**, since SFTP and WebDAV servers sit there next to
  the buckets.

## Under the hood

- Dependencies updated, which closes eight security advisories. Three in the
  TLS library concern certificate authorities that browsers no longer trust.
  The three zip advisories that 0.21.0 left open are gone, now that the Excel
  reader is on its next version. The two in the SSH library concern its
  server side, which Octa does not use.
- Every pull request now builds and tests Octa on Windows as well as on Linux
  and macOS.
- The SSL settings were tested against a real Kubernetes cluster running
  CloudNativePG, reached through `kubectl port-forward`, and against MariaDB
  servers with and without encryption.
- The documentation covers everything above in both the in-app Help and the
  website.
