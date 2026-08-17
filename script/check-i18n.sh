#!/usr/bin/env bash
# Fail if the four I18N packs in app/ui/i18n.js drift apart.
#
# t() falls back to zh-CN on a missing key, so a dropped translation is invisible
# in testing and ships as Chinese text in the English UI. A duplicate key is worse:
# the later literal silently wins and the earlier one is unreachable.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

ruby -e '
src = File.read(ARGV[0])
i = src.index("var I18N") || src.index("const I18N") or abort "I18N object not found"
# End of the object literal: the first line that is exactly two-space "};".
j = nil; pos = i
src[i..-1].each_line { |l| (j = pos + l.length; break) if l =~ /^\s{2}\};\s*$/; pos += l.length }
abort "I18N object never closed" unless j

cur = nil; packs = {}
src[i...j].each_line do |l|
  if l =~ /^\s{4}'"'"'?([a-zA-Z-]+)'"'"'?:\s*\{\s*$/ then cur = $1; packs[cur] = []
  elsif cur && l =~ /^\s{6}'"'"'([^'"'"']+)'"'"':/ then packs[cur] << $1 end
end
abort "parsed no packs" if packs.empty?

fail = false
packs.each do |name, keys|
  seen = Hash.new(0); keys.each { |k| seen[k] += 1 }
  dup = seen.select { |_, c| c > 1 }.keys
  next if dup.empty?
  warn "#{name}: duplicate keys: #{dup.join(", ")}"
  fail = true
end

base = packs["zh-CN"] or abort "no zh-CN pack"
packs.each do |name, keys|
  next if name == "zh-CN"
  miss = base - keys
  extra = keys - base
  warn "#{name}: missing #{miss.size}: #{miss.join(", ")}" unless miss.empty?
  warn "#{name}: not in zh-CN #{extra.size}: #{extra.join(", ")}" unless extra.empty?
  fail = true unless miss.empty? && extra.empty?
end

abort "i18n packs are out of sync" if fail
puts "i18n ok: #{packs.size} packs x #{base.size} keys"
' "$ROOT/app/ui/i18n.js"
