import { SetupWizard } from '@/Manatan/components/SetupWizard';
import { SettingsModal } from '@/Manatan/components/SettingsModal';
import { useOCR } from '@/Manatan/context/OCRContext';

/**
 * Hosts the two global dialogs that belong to the dictionary layer.
 *
 * Upstream rendered these from OCRManager.tsx, together with the manga OCR
 * overlay. P0b deleted that file as OCR-only, which was wrong: it was doing
 * double duty, and taking it out orphaned both the first-run setup wizard and the
 * settings modal that contains dictionary management. The install UI existed in
 * the bundle the whole time with nothing rendering it.
 *
 * Kept separate from App.tsx because both need `useOCR`, which is only available
 * beneath OCRProvider.
 */
export const ManatanHost = () => {
    const { isSettingsOpen, closeSettings } = useOCR();

    return (
        <>
            {/* Opens by itself on first run, until the setup flag is stored. */}
            <SetupWizard />
            {isSettingsOpen && <SettingsModal onClose={closeSettings} />}
        </>
    );
};
