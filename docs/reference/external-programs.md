# Other programs Octa works with

Octa is one program and needs nothing else to open, edit, query and save
files. A few features hand part of their work to another program that you
install yourself. Octa finds each one on your `PATH`, starts it when the
feature needs it, and reads its answer.

None of them is required. When one is missing, only the features in its row
are unavailable, and the app says which program it could not find.

## At a glance

| Program   | From                                | Octa uses it for                                                                                                        |
|-----------|-------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| `git`     | [git-scm.com](https://git-scm.com/) | Comparing a file with an earlier commit, Cell History, and the Git marks in the folder sidebar                          |
| `aws`     | AWS CLI v2                          | Amazon S3 sign-in and bucket list, RDS sign-in with IAM, Amazon Athena credentials                                      |
| `az`      | Azure CLI                           | Azure Blob Storage sign-in and container list, database sign-in with Microsoft Entra ID                                 |
| `gcloud`  | Google Cloud CLI                    | Google Cloud Storage sign-in and bucket list, Cloud SQL sign-in with IAM, BigQuery with Application Default Credentials |
| `ollama`  | [ollama.com](https://ollama.com/)   | Running the assistant on a local model                                                                                  |
| `claude`  | Claude Code CLI                     | The assistant on your Claude plan                                                                                       |
| `copilot` | GitHub Copilot CLI                  | The assistant on your GitHub Copilot plan                                                                               |
| `codex`   | Codex CLI                           | The assistant on your ChatGPT plan                                                                                      |

## What each one does

### Git

Octa calls `git` to read history. It never commits, pushes or changes a
repository.

- [Compare](../usage/view-modes/compare.md) a file with a version from Git.
- [Cell History](../usage/cell-history.md): how a cell or a whole file changed,
  commit by commit.
- Mark changed, new and ignored files in the
  [folder sidebar](../usage/tabs-and-sidebar.md).

Without Git, these entries have nothing to show. Everything else, including
comparing two files or two tabs, works as before.

### AWS, Azure and Google Cloud CLIs

These three do the signing in, so Octa never handles your cloud password or
your second factor. You sign in once with the vendor's own program, in your
browser, and Octa asks that program for a short-lived token when it needs
one.

**Cloud storage** ([Cloud Storage](../usage/cloud-storage.md)):

- The **Sign in** button runs `aws sso login`, `az login` or
  `gcloud auth login`.
- The list of buckets or containers of a connection comes from the CLI.
- An S3 connection that names an **AWS profile** gets its credentials from the
  AWS CLI.

Not needed for: static access keys, an Azure account key or SAS token, a
service-account key, public buckets, and the browser sign-in with your own
OAuth client for Azure Blob and Google Cloud Storage.

**Databases** ([Database Connections](../usage/database-connections.md)):

- **AWS IAM (RDS)**: `aws rds generate-db-auth-token` mints the token for each
  connection.
- **Microsoft Entra ID**: `az account get-access-token`.
- **Google Cloud IAM (Cloud SQL)**: `gcloud sql generate-login-token`.
- **BigQuery with Application Default Credentials**:
  `gcloud auth application-default print-access-token`.
- **Amazon Athena**: `aws configure export-credentials`.

Not needed for: a user name and password, or the other sign-in modes each
engine offers.

How to install them:

| Program  | Windows                                     | macOS                                  | Linux                                                                                              |
|----------|---------------------------------------------|----------------------------------------|----------------------------------------------------------------------------------------------------|
| `aws`    | `winget install -e --id Amazon.AWSCLI`      | `brew install awscli`                  | [AWS install guide](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) |
| `az`     | `winget install -e --id Microsoft.AzureCLI` | `brew install azure-cli`               | [Azure install guide](https://learn.microsoft.com/cli/azure/install-azure-cli-linux)               |
| `gcloud` | `winget install -e --id Google.CloudSDK`    | `brew install --cask google-cloud-sdk` | [Google Cloud install guide](https://cloud.google.com/sdk/docs/install)                            |

### Ollama

With [Ollama](../usage/assistant-setup.md#using-ollama) the assistant runs on
a model on your own computer and nothing leaves it. Octa talks to the Ollama
server, lists the models you have pulled, and can start and stop the server
for you. A server Octa started is stopped when Octa closes.

### Claude Code CLI, GitHub Copilot CLI and Codex CLI

A [subscription profile](../usage/assistant-setup.md#using-a-subscription-instead-of-an-api-key)
runs the assistant on a plan you already pay for, instead of an API key. Octa
starts your own `claude`, `copilot` or `codex` in an empty temporary folder,
hands it one question, and reads the answer. It also asks the program which models it
offers, to fill the Model dropdown. Octa never sees your sign-in.

To show [your plan and its limits](../usage/assistant-setup.md#your-plan-and-its-limits-in-the-panel),
Octa also runs `claude auth status` and, for Codex, starts `codex app-server`
for a moment to ask for the account's limits. Neither sends a question to a
model. Claude reports its limits only beside an answer, so Octa sends it one
fixed three-word question once per start.

The program never runs Octa's tools itself. It asks for one in its answer and
Octa runs it, so a step that uses a tool starts the program once more.

## Parts of your system Octa calls

These come with the operating system. They are listed so that nothing Octa
starts is a surprise.

| What                              | Program                                                                              | When                                                                                                                 |
|-----------------------------------|--------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------|
| Open a link in your browser       | `xdg-open` (Linux), `open` (macOS), `cmd` (Windows)                                  | Help links, browser sign-in, **Open in default app**                                                                 |
| Open a folder in the file manager | `xdg-open`, `open`, `explorer`                                                       | **Open folder** entries and buttons                                                                                  |
| Install an update for all users   | `pkexec` (Linux only)                                                                | When you install an update into a folder that needs administrator rights. It shows the system's own password prompt. |
| Refresh the desktop's icon cache  | `gtk-update-icon-cache`, `xdg-icon-resource`, `update-desktop-database` (Linux only) | After you change the app icon in Settings                                                                            |
| Stop a local model server         | `pkill` (Linux, macOS), `taskkill` (Windows)                                         | When you stop Ollama from the assistant panel                                                                        |

## Good to know

- **Octa looks on your `PATH`.** A program installed only inside WSL is not
  visible to Octa on Windows. Install it on Windows, or run Octa inside WSL
  too.
- **A program started from a desktop icon may see a shorter `PATH`** than your
  terminal does. If a program works in the terminal and Octa cannot find it,
  start Octa from that terminal once to confirm, then add the program's folder
  to the `PATH` your desktop uses. A subscription profile can instead name the
  program's full path.
- **Each program runs under its own terms and privacy policy.** What it sends
  and stores is described by its vendor, not by Octa.
- Separate from these programs, a few readers download a DuckDB extension on
  first use (Delta Lake, Iceberg, Lance). That is a download inside Octa, not
  another program.
