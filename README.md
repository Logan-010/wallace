# Wallace

Wallace is your own local AI voice assistant.
Designed to run on low-end hardware, Wallace can even be run on a raspberry pi.

Inspired by my previous work, and designed to be a successor to, [vobos](https://github.com/Logan-010/vobos).

Wallace itself is a speech to text base model (moonshine) that's fed into a Rig
agent, configured with an LLM provider of your choosing (which supports local
solutions such as Ollama or Llama.CPP). The agent itself has a variety of tools
available such as weather, memory, web search (when a brave web search API key is
present, which is free!), and it's main tool; speech.

Wallace is designed to listen for its name, then speak to you while doing what
you ask.

## Security

Note that since Wallace has access to a `cmd` tool, which lets it run *any* command,
its highly recommended to run Wallace as an unprivileged user.
