# Ассеты и заимствования

## Anna

- Собственный персонаж проекта, кадры сгенерированы ИИ по нашему промпту.
- Исходник — `assets/Anna_source.png` (сетка 4×7, кадр 392 px, чёрный фон). `scripts/key_background.py` делает из него `Resources/Anna.png` с прозрачным фоном; разметка — `Resources/anna.json`.
- Изображения Анны распространяются вместе с проектом на условиях лицензии MIT (см. `LICENSE`).

## WindowPet

- Поведение питомца (`Sources/ClaudePet/PetController.swift`) и формат spritesheet (`anna.json`) повторяют [WindowPet](https://github.com/SeakMengs/WindowPet) (MIT License, Copyright (c) 2023 Seakmeng). Код не копировался — логика переписана на Swift.
