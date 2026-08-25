#!/usr/bin/env bash
# Fail if the four I18N packs in app/qt/qml/I18n.qml drift apart.
#
# t() falls back to zh-CN on a missing key, so a dropped translation is invisible
# in testing and ships as Chinese text in the English UI. A duplicate key is worse:
# the later literal silently wins and the earlier one is unreachable.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/app/qt/qml/I18n.qml"

ruby -e '
src = File.read(ARGV[0])
i = src.index("readonly property var dict") or abort "I18n dict not found"
# End of the dict object: the line that is exactly four-space "})" after packs.
j = nil; pos = i
src[i..-1].each_line { |l| (j = pos + l.length; break) if l =~ /^\s{4}\}\)\s*$/; pos += l.length }
abort "I18n dict never closed" unless j

cur = nil; packs = {}
src[i...j].each_line do |l|
  if l =~ /^\s{8}"([a-zA-Z-]+)":\s*\{\s*$/ then cur = $1; packs[cur] = []
  elsif cur && l =~ /^\s{12}"([^"]+)":/ then packs[cur] << $1
  elsif cur && l =~ /^\s{8}\},?\s*$/ then cur = nil end
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
' "$SRC"
