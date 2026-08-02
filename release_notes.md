# Release notes

This release is mostly about reaching your data without the desktop. Cloud
objects can now be copied, moved and deleted, from the sidebar or the terminal;
Octa can be set up and driven entirely from the command line, including on a
machine with no graphical session at all; the assistant can tell you why a
model profile is not working instead of handing you a raw error; and the
download is smaller.

## Cloud storage: rearrange it, not just read it

**Copy, move and delete objects.** Right-click an object or a folder in the
cloud sidebar for **Copy to...**, **Move to...** or **Delete**. Picking a
folder means everything underneath it. Copying works across providers, so an S3
prefix can go straight into a Google Cloud Storage bucket without a local round
trip. Deleting asks first and says plainly that it cannot be undone.

Writing stays off until you ask for it: the connection needs **Allow writes**
and the global cloud-write switch has to be on, the same two gates that guard
saving a file back.

**The same from the terminal.**

```
octa --cloud-ls s3://bucket/prefix/            # one folder level
octa --cloud-ls s3://bucket/ --recursive       # everything, flattened
octa --cloud-get s3://bucket/data.parquet --out ./data.parquet
octa --cloud-put ./data.parquet --to s3://bucket/data.parquet
octa --cloud-copy s3://a/data/ --to gs://b/backup/
octa --cloud-move s3://a/old.csv --to s3://a/archive/old.csv
octa --cloud-delete s3://a/scratch/ --recursive
```

`--cloud-ls` respects `-f json` and `-f csv` like every other action, so a
bucket listing can be piped into something else.

**Cloud URLs work in the other actions too**, with no download step of your
own:

```
octa --schema s3://bucket/data.parquet
octa --sql s3://bucket/sales.parquet -q 'SELECT count(*) FROM data'
```

**The assistant can move objects as well**, through three new tools:
`copy_object`, `move_object` and `delete_object`. They are write tools, so a
read-only assistant profile and `--mcp-read-only` both drop them.

## Set Octa up without opening it

Until now, adding a cloud bucket or a database meant opening the Settings
dialog, which is awkward on a server and impossible in a container.

**Connections from the command line.**

```
octa --add-connection 'kind=s3,name=prod,bucket=my-bucket,region=eu-central-1,allow_writes=true'
octa --add-connection 'kind=postgres,name=warehouse,host=db.internal,database=analytics,user=reader'
octa --list-connections
octa --remove-connection warehouse
```

**Two environment variables for machines with no desktop.** `OCTA_CONFIG_DIR`
puts the settings file wherever you point it, which a container needs because
it usually has no home directory. `OCTA_NO_KEYRING` stops Octa looking for an
operating-system keyring that is not there, and falls back to the settings
file. The Docker guide covers both.

## The assistant tells you what is wrong

Configuring a model profile used to be guesswork: if it did not work, you got
the provider's raw error, which is often unhelpful.

**Test a profile.** A button next to each profile sends one tiny message with
exactly those settings and shows you what comes back, so you find out in a
second rather than in the middle of a real question.

**Advice instead of an error code.** When a test fails, Octa recognises the
common causes and says what to change: clear the Temperature field for a model
that refuses it, check that an OpenAI-compatible base URL ends in `/v1`, check
the model name in the gateway's own spelling, start the Ollama server, or, when
nothing is misconfigured, tell you the account is simply out of credit.

**Temperature can be left unset.** Newer models, Claude Opus 4.7 and later,
reject the temperature parameter outright. An empty Temperature field now means
the parameter is left out of the request entirely, rather than being sent as
zero.

**Reasoning help per provider.** Hovering the Thinking field now explains what
that particular provider accepts, since they disagree: OpenAI takes an effort
word, current Claude models take an effort word while older ones take a token
budget, and Gemini takes either depending on version.

## A smaller download

**The interface no longer ships a Vulkan stack.** Octa was building on
eframe's default graphics backend, which pulls in a general-purpose GPU
toolchain including a shader compiler and Vulkan bindings. Octa draws
two-dimensional tables and writes no shaders, so it now uses the lighter
OpenGL backend instead: 23 crates leave the build and the picture on screen is
identical, because that layer only uploads finished triangles.

A duplicate SVG renderer went the same way, and the rest of the dependencies
had a general refresh, including the interface toolkit itself.

## Languages

**Serbian reads as Serbian again.** 168 strings were written in Latin script
inside an otherwise Cyrillic interface, so menus changed alphabet halfway
through. Many were also missing their diacritics (`Sacuvaj` rather than
`Sačuvaj`). The whole catalogue is now consistent Cyrillic, apart from format
and product names such as `SQL` and `JSON`, which stay as they are in every
language.

**The folder-union menu entries are translated.** They had shipped in English
in all thirty other languages.

## Smaller things

**A limit for unioning a cloud folder.** **Settings > Performance > Folder
union file cap** sets how many objects a cloud folder union downloads and
merges, defaulting to 500, or removes the limit entirely. Every file is read
fully into memory, so a prefix with tens of thousands of parts could otherwise
exhaust it. Anything past the cap is skipped and counted in the status bar.

**The privacy policy is complete.** It described the assistant, the update
check and map tiles, but not cloud storage or database connections, both of
which send data to the service you configure. Both are now covered, along with
the note that nothing reaches the developer: Octa has no servers of its own.

## Fixes

**Pressing Tab in a text editor no longer risks a crash.** In the raw and
Markdown editors, expanding a Tab into spaces measured the cursor position in
characters but cut the text by bytes. On any line containing a non-English
character the two disagree, so the cursor landed in the wrong place, and if the
offset fell inside a character Octa could stop with a "byte index is not a char
boundary" error. Both editors now count consistently.

**Missing text in R data files is no longer the word "NA".** An absent value in
a character column was shown as the literal text `NA`, which could not be told
apart from a real value of `"NA"`. Such cells are now empty, like missing
values everywhere else.

**The assistant writes where you told it to.** With a profile allowed to write,
a bare filename was resolved against the working directory instead of the
configured export directory.

**Octa introduces itself to other tools as Octa.** When used as an MCP server,
it reported its name as `rmcp`, the library it is built on, so it appeared
under the wrong name in clients such as Claude Desktop.
