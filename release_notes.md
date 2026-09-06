The newest models from OpenAI, Anthropic and Google, model lists that stay in
order instead of growing by accretion, two more OpenAI controls, an Ask box you
can read what you typed in, and Microsoft Store copies that leave updating to
the Store.

## What's new

### The newest models from all three providers

**GPT-6 Astra** (`gpt-6-astra`), **Claude Fable 5.1** (`claude-fable-5-1`) and
**Gemini 3.8 and 3.7 Flash** (`gemini-3.8-flash`, `gemini-3.7-flash`) are in the
model dropdown. Gemini's default moves to `gemini-3.8-flash`, the newest of the
Flash line and the same price class as the one it replaces. The OpenAI and
Anthropic defaults stay on the cheaper models on purpose: a first chat should
not surprise anyone on cost, and the bigger models are one click away.

They reach an existing install too, not only a fresh one, and they arrive at the
top of your list rather than underneath the models they replace.

OpenAI's effort ladder grew with them. The field takes `none`, `minimal`, `low`,
`medium`, `high`, `xhigh` and `max`, and which of those a model accepts is the
model's own business: GPT-6 Astra answers with an error to `none`. Octa passes
the word through untouched, so a level a provider adds tomorrow works today.

### Model lists read newest first, always

The dropdown opens on the most recent model Octa knows about and reads down
through the older ones. This is now kept true rather than merely set once.

A `models.toml` used to grow by accretion: every release appended its new models
to the end, so a file seeded a year ago listed the oldest model first and this
year's flagship last, and only a fresh install ever saw the intended order. Octa
now puts the list back in order when it loads the file.

Names Octa does not ship sit **above** all of them, in the order you had them,
because the reason to type a model name yourself is that it is newer than
Octa's list. Nothing you add is ever removed. One oddity comes with that: a
model Octa shipped in an older release and has since dropped counts as one of
yours as well, so it goes up there too although it is old. Deleting one from the
file sticks, which deleting a current model does not.

Adding and removing names in `models.toml` works exactly as before. Ordering it
by hand is the one thing that will not stick.

Ollama's list, which comes from your local installation rather than the file, is
sorted the same way: most recently pulled first, which is the only sense in
which a local model can be newer than another.

### Answer length and Pro mode, for OpenAI

Two controls appear on an OpenAI profile, and only there, because no other
provider has them.

**Answer length / verbosity** (`low`, `medium` or `high`) sets how long the
visible answer should be. It is a different lever from thinking: effort buys
reasoning before the answer, verbosity buys prose inside it, so "think hard,
answer in one line" is high effort with low verbosity. Leave it empty and the
model decides.

**Pro mode** asks OpenAI for its slower, more thorough reasoning path. The price
per token does not change, it simply spends more of them, so expect a longer
wait and a bigger token count for a harder answer. GPT-5.6 models only; anything
else answers with an error.

### The Ask box in the SQL panel grows with the question

A question longer than the box used to scroll sideways behind itself, so you
could not read what you were typing. The box now starts one line tall and grows
downwards as the text wraps. **Enter** sends the question and **Shift+Enter**
starts a new line, the same as the assistant panel.

### Microsoft Store copies leave updating to the Store

A copy installed from the Microsoft Store is updated by the Store itself, in the
background, and Octa now stays out of it completely: **Help > Check for
updates** is not in the menu, no check runs at launch, and **Settings > Updates
> Check for updates at start** is greyed out and says why.

Octa cannot replace its own files inside `WindowsApps`, so the check could only
ever answer a question nobody had to ask, and inside the package it could not
answer it reliably either: the request to GitHub fails there with a certificate
error, so the entry mostly produced a frightening error about an update Windows
was already installing.

Nothing changes for any other install. Every other way of getting Octa keeps the
full in-app updater, the menu entry and the check at launch.

The release notes are unaffected everywhere, Store included. They ship inside
the copy you installed and cost no request, so an upgrade still announces itself
with the window you are reading now.

## Fixes

- **"Unlimited" response length meant 16,384 tokens on Anthropic.** Anthropic is
  the one provider that requires a token cap, so "unlimited" cannot be expressed
  by leaving the field out the way it is everywhere else, and it became a flat
  16,384. That stopped meaning unlimited a long time ago: the Claude 5 models
  write up to 128,000 tokens, so the setting was quietly costing you most of a
  long answer and truncating tool-heavy replies mid-sentence. Octa now asks the
  API what the chosen model's ceiling actually is and sends that. Answers can
  therefore be longer, and cost more, than they were, which is what the setting
  asked for. If the lookup cannot be made, it falls back to the old number, so
  the worst case is what you had before.
- **Ollama's model list came back in no particular order.** It is sorted by when
  each model was last pulled, newest first, with ties broken by name so the list
  does not shuffle between refreshes. A model with no timestamp stays in the
  list, at the bottom.
- **The reasoning tooltip named five effort levels out of seven**, omitting
  `minimal` and `max`. The tooltip, the in-app Help and the documentation now
  agree with the providers, in all 32 languages.

## Breaking changes

### `models.toml` is put back in order when Octa loads it

The file is still hand-editable and adding or removing model names works exactly
as before. What no longer holds is the **order** you write: Octa rewrites it on
load so the newest models come first, with any name it does not ship kept above
them.

Nothing is deleted by the rewrite. Every name in the file survives; only a name
written twice collapses to one.

## Under the hood

- **The Anthropic token ceiling is read from the API, not from a table in the
  code.** A list of per-model limits is wrong the day a model ships, so Octa
  asks `/v1/models` once per model per session and remembers the answer.
- **The new models needed no new wire format.** Octa already sends Anthropic's
  effort in `output_config`, Gemini's `thinkingLevel` for the 3.x line and a
  numeric budget for 2.5, and already leaves `temperature` out entirely for the
  models that refuse it, which is every current Claude and every OpenAI
  reasoning model.
