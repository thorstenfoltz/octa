# Classify with Jev

**Columns > Classify with Jev...** asks one question about every row and adds
the answer as a new column, with a second column saying how sure the answer
is. The answers come from **Jev**, a decision model by TypeSafe AI.

Jev is not a chat model. It writes no text. It answers a fixed kind of
question about the data you hand it: one of the categories you list, yes or
no, or a grade on a scale you name. That makes it a fit for jobs such as
sorting support messages by topic, flagging rows that describe a complaint, or
grading free-text feedback from poor to excellent.

!!! warning "This sends your data to TypeSafe AI"
    The cells of the columns you choose leave your machine and go to TypeSafe
    AI. Its servers are in the United States only, and it names no period
    after which data is deleted. The feature is therefore **off until you
    switch it on**, and you should send personal or confidential data only if
    you are allowed to. Nothing else in Octa uses this service.

![The Classify with Jev dialog: Load from file and Save to file at the top, the columns to send ticked, the answer kind set to One of my categories, a question, the new column name, three categories with descriptions, the line saying how many values a run sends, and the Send and classify button.](../assets/screenshots/classify-with-jev-dialog.png){ .screenshot-placeholder }

## Switching it on

1. Open **Settings > Classify with Jev**.
2. Read the warning, then tick **Allow sending data to TypeSafe AI**.
3. Paste your TypeSafe API key and click **Save key**. The key goes into the
   system keyring, or into `settings.toml` when the system has none. The
   `TYPESAFE_API_KEY` environment variable takes precedence over both.
4. Click **Refresh** beside **Model**. It asks TypeSafe which models your key
   may use, which is also the quickest way to see that the key works.

Until the switch is on, the menu entry is greyed and says why.

| Setting                | Default      | Notes                                                                |
|------------------------|--------------|----------------------------------------------------------------------|
| **Allow sending data** | off          | The switch. TOML: `enabled` under `[jev]`.                           |
| **Model**              | `jev-latest` | The model that answers. `jev-latest` follows the newest one.         |
| **Parallel requests**  | 4            | Requests sent at once, 1 to 256. Lower it when you hit a rate limit. |

## Running it

1. Open the table and choose **Columns > Classify with Jev...**.
2. Tick the **columns to send**. Their cells are all the model sees of a row,
   so pick only what the question needs.
3. Choose the kind of **answer**:
    - **One of my categories**: list 2 to 255 categories. Each has a name and
      an optional description of what belongs there.
    - **Yes or no**: nothing more to fill in.
    - **A grade on a scale**: list 2 to 10 levels, lowest first.
4. Type the **question** in plain words, for example *What is this message
   about?*
5. Check the line above the buttons. It says how many distinct values a run
   sends and roughly how many tokens that is.
6. Click **Send and classify**. A progress line counts the answers, and
   **Stop** ends the run without changing the table.

## What belongs in which field

### In Settings > Classify with Jev

| Field                                 | What belongs in it                                                                                                                | Example                              |
|---------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------|--------------------------------------|
| **Allow sending data to TypeSafe AI** | A tick, once you have read the warning and may send this data. Everything below stays greyed without it.                          | ticked                               |
| **API key**                           | The key from your TypeSafe account, pasted whole, then **Save key**. **Clear key** deletes the stored one.                        | *(your key)*                         |
| **Model**                             | The name of the model that answers. Leave `jev-latest` unless you need a fixed one. **Refresh** lists the names your key may use. | `jev-latest`                         |
| **Parallel requests**                 | A whole number from 1 to 256: how many requests a run sends at once.                                                              | `4`, or `64` for thousands of values |

### In the dialog

| Field                             | What belongs in it                                                                                                                                                                            | Example                             |
|-----------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------|
| **Load from file...**             | Nothing to type. Pick a classifier you saved earlier and the form is filled from it.                                                                                                          | `classifier.json`                   |
| **Save to file...**               | Nothing to type. Writes the form as it stands to a file.                                                                                                                                      | `support-topics.json`               |
| **Columns to send**               | Ticks on the columns the model has to read to answer. Leave out everything the question does not need, such as ids, names or addresses.                                                       | `subject`, `message`                |
| **Answer**                        | The kind of answer: **One of my categories**, **Yes or no**, or **A grade on a scale**.                                                                                                       | One of my categories                |
| **Question**                      | One question in plain words about a single row. Ask what you would ask a colleague who sees only this row.                                                                                    | *What is this message about?*       |
| **New column**                    | The name of the answer column. It must not exist yet. Left alone it is `category`, `answer` or `score`. The second column takes the same name with `_confidence` or `_probability` behind it. | `topic`                             |
| **Categories**, name              | Only for **One of my categories**. The short label that ends up in the cell. 2 to 255 lines, no name twice.                                                                                   | `billing`                           |
| **Categories**, what belongs here | Optional. A few words telling the model which rows belong to this category. The clearer the borders between categories, the better the answers. Left empty, the name alone has to do.         | *invoices, payments, refunds*       |
| **Levels**                        | Only for **A grade on a scale**. One level per line, 2 to 10, **lowest first**.                                                                                                               | `poor`, `fair`, `good`, `excellent` |

**Yes or no** needs no list. The question itself has to be one that yes or no
answers, for example *Does this message describe a complaint?*

### A worked example

A table of support messages with the columns `id`, `customer`, `subject` and
`message`, to be sorted by topic:

| Field               | Filled in                                             |
|---------------------|-------------------------------------------------------|
| **Columns to send** | `subject`, `message`                                  |
| **Answer**          | One of my categories                                  |
| **Question**        | What is this message about?                           |
| **New column**      | `topic`                                               |
| **Categories**      | `billing`: invoices, payments, refunds                |
|                     | `technical`: errors, crashes, something does not work |
|                     | `account`: sign-in, password, profile data            |
|                     | `other`: anything else                                |

The run adds `topic` and `topic_confidence`. `id` and `customer` never leave
the machine, since they are not ticked. An `other` category is worth having:
without one the model must press every row into a topic that does not fit.

## What you get

Two new columns at the right of the table, added as one undo step:

| Answer kind          | Answer column (default name) | Second column         |
|----------------------|------------------------------|-----------------------|
| One of my categories | `category`, text             | `category_confidence` |
| Yes or no            | `answer`, true or false      | `answer_probability`  |
| A grade on a scale   | `score`, a number            | `score_confidence`    |

The second column is a number from 0 to 1. For yes or no it is the probability
of yes, and the answer is true from 0.5 upwards.

## Saving a classifier

A question you want to ask again, with its columns, categories or levels, can
be kept in a file.

- **Save to file...** writes the form as a small JSON file.
- **Load from file...** fills the form from such a file. Columns are found by
  name, so the file fits any table that has them. Columns the table lacks are
  named under the form and left out.

Nothing is sent by saving or loading. The file holds the question, not your
data and not your key.

## Making a run faster

A run sends **Parallel requests** at once, 4 by default. For thousands of
values, raise it in **Settings > Classify with Jev**, up to 256. How many
TypeSafe accepts depends on your plan. If replies come back with a rate limit,
they are retried after a pause, which makes a run slower again, so lower the
number when that happens.

## Good to know

- **Each distinct value is sent once.** A thousand rows that hold the same
  text cost one request, and all of them get its answer.
- **Empty rows are skipped.** A row whose chosen cells are all empty is not
  sent and stays empty.
- **A value without an answer stays empty.** The dialog stays open and shows
  how many values failed and the first error, which you can select and copy.
  A run in which the first requests all fail stops early, since a wrong key or
  a dead network fails every value the same way.
- **Rate limits are retried.** A rate-limited or overloaded reply is retried a
  few times with a growing pause before it counts as failed.
- **The table must hold still.** If the table changes while the answers are on
  their way, they are not added and the dialog asks you to run it again.
- **A partly loaded tab** classifies the loaded rows only, and says so.
- Closing the dialog stops a run.

## Reporting a label

The labels are written by TypeSafe AI's Jev model, not by Octa, so Octa
cannot change what it answers. The **Report** button at the bottom of the
dialog, or **Help -> Report AI content...**, opens a window with a
**Report to TypeSafe AI** button for a wrong or inappropriate label, and a
second button for problems in Octa itself. See
[Reporting AI content](chatbot.md#reporting-ai-content).

## Without the window

The command line and the MCP server use the same switch and key. For a machine
without a Settings window, put this into `settings.toml`:

```toml
[jev]
enabled = true
```

or set `OCTA_JEV_ENABLED=1`, and give the key as `TYPESAFE_API_KEY`.
`OCTA_JEV_MODEL` overrides the model for one run.

## See also

- Command line: [`--classify`](../cli/classify.md)
- MCP: [`classify_rows`](../mcp/tools/classify_rows.md)
- [Settings reference](../reference/settings.md#classify-with-jev)
