#compdef dtech-aidlc

_dtech-aidlc() {
  local -a scopes harnesses models modes agents results
  scopes=(solo team enterprise)
  harnesses=(claude deepseek)
  models=(opus sonnet haiku fable)
  modes=(human ai)
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
        'onboard:Initialize and detect the project'
        'init:Alias of onboard'
        'scope:Show stages in a scope'
        'status:Show the active intent'
        'doctor:Validate project setup'
        'orchestrate:Advance or report a stage, or park and revisit work units'
        'config:Configure agent models, default scope, mode, gate modes or prerequisites'
        'ids:Check cited identifiers against the ID registry'
        'repos:Manage dependency repositories'
        'deps:Manage workflow dependency edges'
        'intent:List, pause, resume, abandon, rework or split intents'
        'knowledge:Manage project knowledge and sources'
        'harness:Inspect and refresh projected skill files'
        'completion:Print shell completion'
      )
      _describe 'command' commands
      ;;
    args)
      case ${words[1]} in
        init|onboard)
          _arguments \
            '--harness[Coding harness]:harness:(claude deepseek)' \
            '--scope[Workflow scope]:scope:(solo team enterprise)' \
            '--rescan[Re-scan an initialized project]' \
            '--docs[Documentation path]:path:_files' \
            '--no-detect[Skip automatic documentation detection]'
          ;;
        scope)
          _arguments '--depth[Depth]:depth:(full standard fast)' '1:scope:(solo team enterprise)'
          ;;
        status)
          _arguments '--intent[Intent id]:intent:'
          ;;
        knowledge)
          _arguments -C '1:action:(init sources list show check index)' '*::arg:->knowledgerest'
          case $state in
            knowledgerest)
              case ${words[1]} in
                sources) _arguments '1:sub:(list add remove)' '--kind[Source kind]:kind:(project docs requirements architecture adr api-spec runbook conventions readme)' '--description[Description]:description:' '2:id:' '3:path:' ;;
                show) _arguments '1:id:' ;;
              esac
              ;;
          esac
          ;;
        intent)
          _arguments -C '1:action:(list pause resume abandon depth rework split assign)' '*::arg:->intentrest'
          case $state in
            intentrest)
              case ${words[1]} in
                pause) _arguments '--detail[Reason]:detail:' ;;
                resume) _arguments '1:id:' ;;
                abandon) _arguments '--detail[Reason]:detail:' '1:id:' ;;
                depth) _arguments '--approved-by[Approver]:name:' '--detail[Reason]:detail:' '1:depth:(full standard fast)' ;;
                rework) _arguments '--unit[Work unit to send back to design]:unit:' '--detail[Reason]:detail:' ;;
                split) _arguments '1:unit:' '--into[Work units replacing it]:units:' '--detail[Reason]:detail:' ;;
                assign) _arguments '1:name:' ;;
              esac
              ;;
          esac
          ;;
        orchestrate)
          _arguments -C '1:action:->action' '*::arg:->rest'
          case $state in
            action) _values 'action' next report park queue revisit ;;
            rest)
              case ${words[1]} in
                next) _arguments '--intent[Intent text]:intent:' '--scope[Workflow scope]:scope:(solo team enterprise)' '--depth[Depth]:depth:(full standard fast)' '--mode[Orchestration mode]:mode:(human ai)' '--approved-by[Approver]:name:' '--detail[Reason]:detail:' '--force-resume[Override a failing prerequisite check]' '--claim[Claim one more ready work unit]' '--unit[Work unit in flight]:unit:' ;;
                report)
                  _arguments \
                    '--stage[Stage identifier]:stage:' \
                    '--result[Stage result]:result:(complete blocked skipped)' \
                    '--approval[Phase approval]:approval:(approved)' \
                    '--needs[Named prerequisite that blocks the stage]:name:' \
                    '--unit[Work unit being reported (the current unit)]:unit:' \
                    '--detail[Result detail]:detail:'
                  ;;
                park)
                  _arguments \
                    '--unit[Work unit to park (the current unit)]:unit:' \
                    '--kind[Why the unit is blocked]:kind:(dep-unit foreign-regression owner-decision design-defect)' \
                    '--blocked-by[Unit, test or decision it waits for]:id:' \
                    '--detail[Reason]:detail:'
                  ;;
                revisit)
                  _arguments \
                    '--unit[Parked work unit to return to the queue]:unit:' \
                    '--detail[Decision or reason]:detail:' \
                    '--approved-by[Who decided]:name:'
                  ;;
              esac
              ;;
          esac
          ;;
        harness)
          _arguments -C '1:action:(add status refresh)' '--dry-run[Show planned changes]' '--force[Back up and replace edited files]' '1:name:(claude deepseek)'
          ;;
        config)
          _arguments -C '1:section:(agents scope mode gate-mode prerequisite)' '2:action:->action' '*::arg:->rest'
          case ${words[1]} in
            mode) _arguments '1:mode:(human ai)' ;;
            scope) _arguments '1:scope:(solo team enterprise)' ;;
            gate-mode) _arguments '1:gate:(analysis-baseline solution-direction release-candidate launch-and-handoff closure intent-closure)' '2:mode:(human default)' ;;
            prerequisite) _arguments '1:action:(set remove check)' '2:name:' '--check[Check command]:command:' '--hint[Hint shown when the check fails]:hint:' '--applies-to[Stage or phase to preflight]:stage:' ;;
          esac
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
        ids)
          _arguments '1:action:(check)' '--stage[Stage identifier]:stage:'
          ;;
        completion)
          _arguments '1:shell:(zsh)'
          ;;
      esac
      ;;
  esac
}

_dtech-aidlc "$@"
