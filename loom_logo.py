#!/usr/bin/env python3
"""LOOM logo: white text on #663BF9 purple background."""
import sys

PURPLE = "\033[48;2;102;59;249m"
WHITE  = "\033[38;2;255;255;255m"
RESET  = "\033[0m"

# Each letter: 7 cols wide, 9 rows tall
# Spacing: 2 cols between letters
# Total LOOM width: 7+2+7+2+7+2+7 = 34
# Canvas: 50 cols × 15 rows (center LOOM)

L = [
    "█      ",
    "█      ",
    "█      ",
    "█      ",
    "█      ",
    "█      ",
    "█      ",
    "█      ",
    "███████",
]

O = [
    " █████ ",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
    " █████ ",
]

M = [
    "█     █",
    "██   ██",
    "█ █ █ █",
    "█  █  █",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
    "█     █",
]

PAD_TOP = 3
PAD_BOT = 3
WIDTH = 50
LOOM_WIDTH = 7 + 2 + 7 + 2 + 7 + 2 + 7  # 34
LEFT_PAD = (WIDTH - LOOM_WIDTH) // 2      # 8

def render_row(letter_row_idx):
    """Render one row of LOOM."""
    gap = "  "
    return L[letter_row_idx] + gap + O[letter_row_idx] + gap + O[letter_row_idx] + gap + M[letter_row_idx]

for _ in range(PAD_TOP):
    print(PURPLE + " " * WIDTH + RESET)

for i in range(9):
    row = render_row(i)
    line = PURPLE + " " * LEFT_PAD + WHITE + row + RESET + PURPLE + " " * LEFT_PAD + RESET
    print(line)

for _ in range(PAD_BOT):
    print(PURPLE + " " * WIDTH + RESET)

print()
