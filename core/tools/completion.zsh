#compdef dtech-aidlc

_dtech-aidlc() {
  local -a scopes harnesses models agents results
  scopes=(solo team enterprise)
  harnesses=(claude deepseek)
  models=(opus sonnet haiku fable)
  agents=(
    brief-analyst context-risk-analyst
    solution-explorer decision-facilitator
    implementation-planner builder-verifier
    release-coordinator operations-handoff
    outcome-curator knowledge-steward
    record-keeper knowledge-archivist
  )
  results=(complete blocked skipped)

  _arguments -C \
    '--project[Project directory]:directory:_directories' \
    '--help[Show help]' \
    '--version[Show version]' \
    '1:command:->command' \
    '*::arg:->args'

  case $state in
    command)
      local -a commands
      commands=(
        'help:Show help'
        'version:Show version'
        'init:Initialize the project'
        'scope:Show stages in a scope'
        'status:Show the active intent'
        'doctor:Validate project setup'
        'orchestrate:Advance or report a stage'
        'config:Configure agent models'
        'repos:Manage dependency repositories'
        'deps:Manage workflow dependency edges'
        'completion:Print shell completion'
      )
      _describe 'command' commands
      ;;
    args)
      case ${words[1]} in
        init)
          _arguments \
            '--harness[Coding harness]:harness:(claude deepseek)' \
            '--scope[Workflow scope]:scope:(solo team enterprise)'
          ;;
        scope)
          _arguments '1:scope:(solo team enterprise)'
          ;;
        orchestrate)
          _arguments -C '1:action:->action' '*::arg:->rest'
          case $state in
            action) _values 'action' next report ;;
            rest)
              case ${words[1]} in
                next) _arguments '--intent[Intent text]:intent:' '--scope[Workflow scope]:scope:(solo team enterprise)' ;;
                report)
                  _arguments \
                    '--stage[Stage identifier]:stage:' \
                    '--result[Stage result]:result:(complete blocked skipped)' \
                    '--approval[Phase approval]:approval:(approved)' \
                    '--detail[Result detail]:detail:'
                  ;;
              esac
              ;;
          esac
          ;;
        config)
          _arguments -C '1:section:(agents)' '2:action:->action' '*::arg:->rest'
          case ${words[2]} in
            set) _arguments '1:agent:(brief-analyst context-risk-analyst solution-explorer decision-facilitator implementation-planner builder-verifier release-coordinator operations-handoff outcome-curator knowledge-steward record-keeper knowledge-archivist)' '2:model:(opus sonnet haiku fable)' ;;
            reset) _arguments '1:agent:(brief-analyst context-risk-analyst solution-explorer decision-facilitator implementation-planner builder-verifier release-coordinator operations-handoff outcome-curator knowledge-steward record-keeper knowledge-archivist)' ;;
          esac
          ;;
        repos)
          _arguments -C '1:action:->action' '*::arg:->rest'
          case $state in
            action) _values 'action' add list pull remove ;;
            rest)
              case ${words[1]} in
                add) _arguments '--ref[Git ref]:ref:' '1:name:' '2:url:_urls' ;;
                pull) _arguments '--file[Dependency JSON file]:file:_files' '1:name:' ;;
                remove) _arguments '1:name:' ;;
              esac
              ;;
          esac
          ;;
        deps)
          _arguments '1:action:(add list pull)' '2:id:' '3:dependency:'
          ;;
        completion)
          _arguments '1:shell:(zsh)'
          ;;
      esac
      ;;
  esac
}

_dtech-aidlc "$@"
