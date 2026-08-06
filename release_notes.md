# Release notes

This release adds a way to report what the assistant writes.

## Reporting what the assistant writes

The assistant shows you text written by a language model. Octa neither runs nor
trains that model: you bring your own API key, or run one on your own machine
with Ollama. So when a reply is inappropriate, Octa cannot change it, and the
complaint needs to reach whoever actually serves the model.

**A Report button in the assistant.** The panel header has a **Report** button,
and there is a **Report AI content...** entry in the **Help** menu for when the
panel is closed. Either one opens a short dialog that names the profile you are
using and points the report where it can be acted on:

- a **hosted model** (Claude, GPT, Gemini) links to that provider's support
  channel,
- **Ollama** runs on your own computer, so the dialog says so and names the
  people who published the model you downloaded instead,
- an **OpenAI-compatible** endpoint shows the URL you configured, since whoever
  operates it is the one to tell.

**A second button for Octa itself.** Underneath, and always present, is a
button for problems that are Octa's fault rather than the model's: a reply
displayed wrong, a tool doing something unexpected, the panel misbehaving.
That one opens an issue on Octa's own tracker, and those do get fixed.

**Why it exists.** The Microsoft Store requires any product that presents
generative-AI output to offer a way of reporting it, and the assistant presents
it. That is a condition of publishing in the Store rather than anything the law
demands, and strictly it binds only the Store build. The button ships in every
build regardless: there is no sense in one version of Octa knowing where to
send a complaint and another not.

The dialog is translated into all thirty-two interface languages.
