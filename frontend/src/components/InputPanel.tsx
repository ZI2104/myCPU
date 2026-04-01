import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

type InputKey = 'up' | 'left' | 'down' | 'right' | 'a' | 'b';

type InputPanelProps = {
  sendCommand: (command: string) => void;
};

const INITIAL_STATE: Record<InputKey, boolean> = {
  up: false,
  left: false,
  down: false,
  right: false,
  a: false,
  b: false,
};

const KEYBOARD_MAPPING: Record<string, InputKey> = {
  ArrowUp: 'up',
  KeyW: 'up',
  ArrowLeft: 'left',
  KeyA: 'left',
  ArrowDown: 'down',
  KeyS: 'down',
  ArrowRight: 'right',
  KeyD: 'right',
  KeyJ: 'a',
  Enter: 'a',
  KeyK: 'b',
  ShiftRight: 'b',
};

const INPUT_KEYS: InputKey[] = ['up', 'left', 'down', 'right', 'a', 'b'];

export const InputPanel: React.FC<InputPanelProps> = ({ sendCommand }) => {
  const [pressed, setPressed] = useState<Record<InputKey, boolean>>(INITIAL_STATE);
  const pressedRef = useRef<Record<InputKey, boolean>>(INITIAL_STATE);

  const setKeyPressed = useCallback(
    (key: InputKey, isPressed: boolean) => {
      const current = pressedRef.current[key];
      if (current === isPressed) {
        return;
      }

      const next = {
        ...pressedRef.current,
        [key]: isPressed,
      };
      pressedRef.current = next;
      setPressed(next);
      sendCommand(`input ${key} ${isPressed ? 'down' : 'up'}`);
    },
    [sendCommand],
  );

  const releaseAll = useCallback(() => {
    const activeKeys = INPUT_KEYS.filter((key) => pressedRef.current[key]);
    if (activeKeys.length === 0) {
      return;
    }

    activeKeys.forEach((key) => {
      sendCommand(`input ${key} up`);
    });

    pressedRef.current = { ...INITIAL_STATE };
    setPressed({ ...INITIAL_STATE });
    sendCommand('input clear');
  }, [sendCommand]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mapped = KEYBOARD_MAPPING[event.code];
      if (!mapped) {
        return;
      }
      event.preventDefault();
      if (event.repeat) {
        return;
      }
      setKeyPressed(mapped, true);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      const mapped = KEYBOARD_MAPPING[event.code];
      if (!mapped) {
        return;
      }
      event.preventDefault();
      setKeyPressed(mapped, false);
    };

    const onBlur = () => {
      releaseAll();
    };

    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);

    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
      releaseAll();
    };
  }, [releaseAll, setKeyPressed]);

  const hint = useMemo(
    () => '键盘: WASD/方向键 + J(确认)/K(返回)，或直接点击按钮。',
    [],
  );

  return (
    <div className="input-panel">
      <div className="input-panel-header">
        <h3>Input Panel</h3>
        <button className="input-reset" onClick={releaseAll}>
          Release All
        </button>
      </div>

      <div className="input-grid">
        <button
          className={`input-btn dpad ${pressed.up ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('up', true)}
          onMouseUp={() => setKeyPressed('up', false)}
          onMouseLeave={() => setKeyPressed('up', false)}
        >
          ↑
        </button>
        <button
          className={`input-btn dpad ${pressed.left ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('left', true)}
          onMouseUp={() => setKeyPressed('left', false)}
          onMouseLeave={() => setKeyPressed('left', false)}
        >
          ←
        </button>
        <button
          className={`input-btn dpad ${pressed.down ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('down', true)}
          onMouseUp={() => setKeyPressed('down', false)}
          onMouseLeave={() => setKeyPressed('down', false)}
        >
          ↓
        </button>
        <button
          className={`input-btn dpad ${pressed.right ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('right', true)}
          onMouseUp={() => setKeyPressed('right', false)}
          onMouseLeave={() => setKeyPressed('right', false)}
        >
          →
        </button>

        <button
          className={`input-btn action ${pressed.a ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('a', true)}
          onMouseUp={() => setKeyPressed('a', false)}
          onMouseLeave={() => setKeyPressed('a', false)}
        >
          A
        </button>
        <button
          className={`input-btn action ${pressed.b ? 'pressed' : ''}`}
          onMouseDown={() => setKeyPressed('b', true)}
          onMouseUp={() => setKeyPressed('b', false)}
          onMouseLeave={() => setKeyPressed('b', false)}
        >
          B
        </button>
      </div>

      <div className="input-hint">{hint}</div>
    </div>
  );
};
