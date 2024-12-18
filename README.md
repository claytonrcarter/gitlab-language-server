# gitlab-language-server

A [language server][1] to provide completions for common GitLab resources in
Markdown documents. Powered by Rust.

## Features

- completion suggestions for project members, milestones, labels and (some)
  quick actions

## Configuration

The following client-side configuration options are supported:

- `project`: (**required**) the name of the project to query

For example, in Zed, these could be set in your `settings.json`, like so:

```json
{
  ...

  "lsp": {
    "gitlab-language-server": {
      "initialization_options": {
        "project": "username/projext",
      }
    }
  },

  ...
}
```

## Comparison

This differs from [official GitLab language server][2] in that it only focuses
on completions, not on AI integration; and from [gitlab-ci-ls][3] in that it
doesn't focus on CI.

[1]: https://microsoft.github.io/language-server-protocol/
[2]: https://gitlab.com/gitlab-org/editor-extensions/gitlab-lsp
[3]: https://github.com/alesbrelih/gitlab-ci-ls
