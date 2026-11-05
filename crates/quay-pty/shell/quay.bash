# quay-term shell integration for bash: marks prompts and commands with OSC 133.
# Started from a one-shot PROMPT_COMMAND set by quay-term, so it removes that bootstrap first.
[[ $- == *i* ]] || return 0
if [[ -n ${QUAY_SHELL_INTEGRATION_DIR-} ]]; then
  __quay_boot=". '${QUAY_SHELL_INTEGRATION_DIR//\'/\'\\\'\'}/quay.bash'"
  PROMPT_COMMAND=${PROMPT_COMMAND/"$__quay_boot"/}
  PROMPT_COMMAND=${PROMPT_COMMAND#;}
  unset __quay_boot QUAY_SHELL_INTEGRATION_DIR
fi
[[ -n ${__quay_integrated-} ]] && return 0
__quay_integrated=1
__quay_prompt() { local e=$?; printf '\e]133;D;%s\a\e]133;A\a' "$e"; }
__quay_prompt_end() { [[ $PS1 == *133?B* ]] || PS1+='\[\e]133;B\a\]'; }
PROMPT_COMMAND="__quay_prompt;${PROMPT_COMMAND:+$PROMPT_COMMAND;}__quay_prompt_end"
# PS0 (bash 4.4+) marks the start of command output; older bash simply gets no command blocks.
PS0='\e]133;C\a'
