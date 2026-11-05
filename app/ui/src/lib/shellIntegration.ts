/**
 * One line a user can run in a remote bash or zsh to get the same prompt and command marks that
 * local shells get automatically. It is typed into the pane only when the user asks for it, and
 * it writes nothing to the remote machine's disk: it only defines shell functions for that
 * session. The leading space keeps it out of history in shells that ignore space-prefixed lines.
 */
export const REMOTE_SNIPPET =
  ` if [ -n "$ZSH_VERSION" ]; then ` +
  `__q(){ printf '\\e]133;D;%s\\a\\e]133;A\\a' $?; }; __qc(){ printf '\\e]133;C\\a'; }; __qe(){ case $PS1 in *133?B*) ;; *) PS1=$PS1$'%{\\e]133;B\\a%}';; esac; }; ` +
  `precmd_functions=(__q $precmd_functions __qe); preexec_functions+=(__qc); ` +
  `elif [ -n "$BASH_VERSION" ]; then ` +
  `__q(){ printf '\\e]133;D;%s\\a\\e]133;A\\a' $?; }; __qe(){ case $PS1 in *133?B*) ;; *) PS1=$PS1'\\[\\e]133;B\\a\\]';; esac; }; ` +
  `PROMPT_COMMAND="__q;\${PROMPT_COMMAND:+$PROMPT_COMMAND;}__qe"; PS0='\\e]133;C\\a'; ` +
  `fi\r`;
