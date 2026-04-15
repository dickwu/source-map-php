<?php

namespace Tests\Feature;

use PHPUnit\Framework\Attributes\CoversMethod;
use App\Services\ConsentService;

class PatientConsentTest
{
    #[CoversMethod(ConsentService::class, 'sign')]
    public function it_signs_consent(): void
    {
        $this->post('/patients/fixture/consents');
    }
}
