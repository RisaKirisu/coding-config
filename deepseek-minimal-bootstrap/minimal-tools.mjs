import { readdir, readFile, stat, writeFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative } from 'node:path'

// DeepSeek Harness Minimal preset at 47f943859bef60e4160492346772ded9b24f765a.
export const MINIMAL_BASH_DESCRIPTION = [
  'Run commands in a bash shell',
  '* When invoking this tool, the contents of the "command" parameter does NOT need to be XML-escaped.',
  "* You don't have access to the internet via this tool.",
  '* You do have access to a mirror of common linux and python packages via apt and pip.',
  '* State is persistent across command calls and discussions with the user.',
  "* To inspect a particular line range of a file, e.g. lines 10-25, try 'sed -n 10,25p /path/to/the/file'.",
  '* Please avoid commands that may produce a very large amount of output.',
  "* Please run long lived commands in the background, e.g. 'sleep 10 &' or start a server in the background.",
].join('\n')

export const STR_REPLACE_EDITOR_DESCRIPTION = [
  'Custom editing tool for viewing, creating and editing files',
  '* State is persistent across command calls and discussions with the user',
  '* If `path` is a file, `view` displays the result of applying `cat -n`. If `path` is a directory, `view` lists non-hidden files and directories up to 2 levels deep',
  '* The `create` command cannot be used if the specified `path` already exists as a file',
  '* If a `command` generates a long output, it will be truncated and marked with `<response clipped>`',
  '',
  'Notes for using the `str_replace` command:',
  '* The `old_str` parameter should match EXACTLY one or more consecutive lines from the original file. Be mindful of whitespaces!',
  '* If the `old_str` parameter is not unique in the file, the replacement will not be performed. Make sure to include enough context in `old_str` to make it unique',
  '* The `new_str` parameter should contain the edited lines that should replace the `old_str`',
].join('\n')

export const MINIMAL_TOOL_DEFINITIONS = [
  {
    type: 'function',
    function: {
      name: 'bash',
      description: MINIMAL_BASH_DESCRIPTION,
      parameters: {
        type: 'object',
        properties: {
          command: {
            type: 'string',
            description: 'The bash command to run. Relative path is preferred in the command.',
          },
        },
        required: ['command'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'str_replace_editor',
      description: STR_REPLACE_EDITOR_DESCRIPTION,
      parameters: {
        type: 'object',
        properties: {
          command: {
            type: 'string',
            description: 'The commands to run. Allowed options are: `view`, `create`, `str_replace`, `insert`.',
            enum: ['view', 'create', 'str_replace', 'insert'],
          },
          path: {
            type: 'string',
            description: 'Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.',
          },
          file_text: {
            type: 'string',
            description: 'Required parameter of `create` command, with the content of the file to be created.',
          },
          insert_line: {
            type: 'integer',
            description: 'Required parameter of `insert` command. The `new_str` will be inserted AFTER the line `insert_line` of `path`.',
          },
          new_str: {
            type: 'string',
            description: 'Optional parameter of `str_replace` command containing the new string (if not given, no string will be added). Required parameter of `insert` command containing the string to insert.',
          },
          old_str: {
            type: 'string',
            description: 'Required parameter of `str_replace` command containing the string in `path` to replace.',
          },
          view_range: {
            type: 'array',
            description: 'Optional parameter of `view` command when `path` points to a file. If none is given, the full file is shown. If provided, the file will be shown in the indicated line number range, e.g. [11, 12] will show lines 11 and 12. Indexing at 1 to start. Setting `[start_line, -1]` shows all lines from `start_line` to the end of the file.',
            items: { type: 'integer' },
          },
        },
        required: ['command', 'path'],
      },
    },
  },
]

const MAX_OUTPUT_CHARS = 16_000
const TRUNCATED_MESSAGE = '<response clipped><NOTE>To save on context only part of this file has been shown to you. You should retry this tool after you have searched inside the file with `grep -n` in order to find the line numbers of what you are looking for.</NOTE>'

function truncate(content) {
  return content.length <= MAX_OUTPUT_CHARS
    ? content
    : content.slice(0, MAX_OUTPUT_CHARS) + TRUNCATED_MESSAGE
}

function required(value, parameter, command, allowEmpty = true) {
  if (value === undefined) throw new Error(`Parameter \`${parameter}\` is required for command: ${command}`)
  if (typeof value !== 'string') throw new TypeError(`Parameter \`${parameter}\` must be a string`)
  if (!allowEmpty && value.length === 0) throw new Error(`Parameter \`${parameter}\` is empty for command: ${command}`)
  return value
}

function targetPath(value) {
  if (typeof value !== 'string' || !isAbsolute(value)) {
    throw new Error('The `path` parameter must be an absolute path.')
  }
  return value
}

function within(root, target) {
  const rel = relative(root, target)
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel))
}

async function authorize(context, target, permission) {
  const root = context.worktree ?? context.directory
  if (root && !within(root, target)) {
    const pattern = join(dirname(target), '*').replaceAll('\\', '/')
    await context.ask({
      permission: 'external_directory',
      patterns: [pattern],
      always: [pattern],
      metadata: { filepath: target, parentDir: dirname(target) },
    })
  }
  await context.ask({
    permission,
    patterns: [root ? relative(root, target) : target],
    always: ['*'],
    metadata: { filepath: target },
  })
}

function formatFileView(path, content, viewRange) {
  const allLines = content.split('\n')
  let lines = allLines
  let initialLine = 1
  let finalLine
  let prompt = `Here's the content of ${path} with line numbers (which has a total of ${allLines.length} lines)`

  if (viewRange !== undefined) {
    if (!Array.isArray(viewRange) || viewRange.length !== 2 || !viewRange.every(Number.isInteger)) {
      throw new Error('Invalid `view_range`. It should be a list of two integers.')
    }
    ;[initialLine, finalLine] = viewRange
    if (initialLine < 1 || initialLine > allLines.length) {
      throw new Error(`Invalid \`view_range\`: [${viewRange.join(', ')}]. Its first element \`${initialLine}\` should be within the range of lines of the file: [1, ${allLines.length}]`)
    }
    if (finalLine > allLines.length) {
      throw new Error(`Invalid \`view_range\`: [${viewRange.join(', ')}]. Its second element \`${finalLine}\` should be smaller than the number of lines in the file: \`${allLines.length}\``)
    }
    if (finalLine !== -1 && finalLine < initialLine) {
      throw new Error(`Invalid \`view_range\`: [${viewRange.join(', ')}]. Its second element \`${finalLine}\` should be larger or equal than its first \`${initialLine}\``)
    }
    lines = finalLine === -1 ? allLines.slice(initialLine - 1) : allLines.slice(initialLine - 1, finalLine)
    prompt += ` with view_range=[${initialLine}, ${finalLine}]`
  }

  const numbered = lines.map((line, index) => `${String(initialLine + index).padStart(6, ' ')}  ${line}`).join('\n')
  return truncate(`${prompt}:\n${numbered}\n`)
}

async function listDirectory(path, signal) {
  const rows = [`d\t${path}`]
  async function visit(directory, depth) {
    signal.throwIfAborted()
    const entries = await readdir(directory, { withFileTypes: true })
    for (const entry of entries) {
      if (entry.name.startsWith('.') || entry.name === 'node_modules' || entry.name === '__pycache__') continue
      const child = join(directory, entry.name)
      const type = entry.isDirectory() ? 'd' : entry.isFile() ? 'f' : '?'
      rows.push(`${type}\t${child}`)
      if (entry.isDirectory() && depth < 2) await visit(child, depth + 1)
    }
  }
  await visit(path, 1)
  rows.sort((left, right) => left.slice(left.indexOf('\t') + 1).localeCompare(right.slice(right.indexOf('\t') + 1)))
  const listing = truncate(rows.join('\n') + '\n')
  return `Here're the files and directories up to 2 levels deep in ${path}, excluding hidden items, node_modules, and Python cache directories:\n${listing}\n`
}

function offsets(content, search) {
  const found = []
  for (let offset = content.indexOf(search); offset >= 0; offset = content.indexOf(search, offset + search.length)) {
    found.push(offset)
  }
  return found
}

function lineNumbers(content, found) {
  return found.map(offset => content.slice(0, offset).split('\n').length)
}

export function createStrReplaceEditorTool() {
  return {
    description: STR_REPLACE_EDITOR_DESCRIPTION,
    // OpenCode 1.18.18's legacy JSON Schema bridge treats every declared field
    // as required. Optional fields remain accepted as additional properties.
    args: {
      command: MINIMAL_TOOL_DEFINITIONS[1].function.parameters.properties.command,
      path: MINIMAL_TOOL_DEFINITIONS[1].function.parameters.properties.path,
    },
    async execute(args, context) {
      context.abort.throwIfAborted()
      const path = targetPath(args.path)
      const command = args.command
      context.metadata({ title: `${command} ${path}` })

      if (command === 'view') {
        await authorize(context, path, 'read')
        const info = await stat(path).catch((error) => {
          if (error?.code === 'ENOENT') throw new Error(`The path ${path} does not exist. Please provide a valid path.`)
          throw error
        })
        if (info.isDirectory()) {
          if (args.view_range !== undefined) throw new Error('The `view_range` parameter is not allowed when `path` points to a directory.')
          return listDirectory(path, context.abort)
        }
        if (!info.isFile()) throw new Error(`cannot view "${path}": not a regular file or directory`)
        return formatFileView(path, await readFile(path, { encoding: 'utf8', signal: context.abort }), args.view_range)
      }

      await authorize(context, path, 'edit')
      if (command === 'create') {
        const fileText = required(args.file_text, 'file_text', 'create')
        await writeFile(path, fileText, { encoding: 'utf8', flag: 'wx', signal: context.abort }).catch((error) => {
          if (error?.code === 'EEXIST') throw new Error(`File already exists at: ${path}. Cannot overwrite files using command \`create\`.`)
          throw error
        })
        return `New file created successfully at: ${path}`
      }

      const before = await readFile(path, { encoding: 'utf8', signal: context.abort })
      if (command === 'str_replace') {
        const oldValue = required(args.old_str, 'old_str', 'str_replace', false)
        const found = offsets(before, oldValue)
        if (found.length === 0) throw new Error(`No replacement was performed, old_str \`${oldValue}\` did not appear verbatim in ${path}.`)
        if (found.length > 1) {
          throw new Error(`No replacement was performed. Multiple occurrences of old_str \`${oldValue}\` in lines [${lineNumbers(before, found).join(', ')}]. Please ensure it is unique`)
        }
        const after = before.slice(0, found[0]) + (args.new_str ?? '') + before.slice(found[0] + oldValue.length)
        await writeFile(path, after, { encoding: 'utf8', signal: context.abort })
        return `The file ${path} has been edited successfully.`
      }

      if (command === 'insert') {
        const insertLine = args.insert_line
        if (insertLine === undefined) throw new Error('Parameter `insert_line` is required for command: insert')
        const value = required(args.new_str, 'new_str', 'insert')
        const lines = before.split('\n')
        if (!Number.isInteger(insertLine) || insertLine < 0 || insertLine > lines.length) {
          throw new Error(`Invalid \`insert_line\` parameter: ${insertLine}. It should be within the range of lines of the file: [0, ${lines.length}]`)
        }
        const after = [...lines.slice(0, insertLine), ...value.split('\n'), ...lines.slice(insertLine)].join('\n')
        await writeFile(path, after, { encoding: 'utf8', signal: context.abort })
        return `The file ${path} has been edited successfully.`
      }

      throw new Error('The `command` parameter must be one of: `view`, `create`, `str_replace`, `insert`.')
    },
  }
}
